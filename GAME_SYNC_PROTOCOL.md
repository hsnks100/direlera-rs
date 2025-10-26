# Kaillera Game Synchronization Protocol

This document describes the **actual behavior** of the Kaillera protocol's game synchronization mechanism, discovered through reverse engineering and packet analysis.

The original protocol documentation only provided basic packet formats without explaining critical details like per-player caching, frame interleaving, or multi-delay synchronization. These mechanisms were discovered by analyzing real Kaillera server traffic with Wireshark.

## Table of Contents

1. [Packet Formats](#packet-formats)
2. [Cache System](#cache-system)
3. [Player Delays](#player-delays)
4. [Frame Synchronization](#frame-synchronization)
5. [Preemptive Padding](#preemptive-padding)
6. [Frame Distribution](#frame-distribution)
7. [Sequence Diagrams](#sequence-diagrams)

---

## Packet Formats

### Game Data (0x12)

**Client → Server:**

```
+------+------+-------------------+
| 0x12 | 0x00 | Input Data        |
+------+------+-------------------+
  1B     1B     N bytes (N = delay × 2)
```

**Server → Client:**

```
+------+------+----------------------------+
| 0x12 | 0x00 | Combined Data              |
+------+------+----------------------------+
  1B     1B     player_count × delay × 2 bytes
```

### Game Cache (0x13)

**Client → Server:**

```
+------+------+----------+
| 0x13 | 0x00 | Position |
+------+------+----------+
  1B     1B       1B
```

**Server → Client:**

```
+------+------+----------+
| 0x13 | 0x00 | Position |
+------+------+----------+
  1B     1B       1B
```

---

## Cache System

### Architecture

- **256-slot FIFO cache** (positions 0-255, wraps around)
- **Client input cache**: Stores inputs the client has sent
- **Server output cache (per player)**: Stores combined data each player has received

### Cache Behavior

**Sending:**

```
IF current_data exists in cache at position P:
    Send Game Cache(P)
ELSE:
    Send Game Data(data)
    cache[next_position] = data
    next_position = (next_position + 1) % 256
```

**Receiving Game Cache:**

```
data = cache[received_position]
Process data
```

### Per-Player Server Caches

The server maintains **separate output caches for each player**.

Example:

```
Frame 1: P0 receives [A1 A2 B1 B2] → P0's cache position 5

Frame 5: Server sends [A1 A2 B1 B2] again
  → P0: Game Cache(5)
  → P1: Game Data([A1 A2 B1 B2])  (P1 hasn't seen this data)
```

---

## Player Delays

### Definition

| Delay | Send Rate (60fps)        | Input Size  |
| ----- | ------------------------ | ----------- |
| 1     | Every frame (~16.7ms)    | 2 bytes     |
| 2     | Every 2 frames (~33.3ms) | 4 bytes     |
| 3     | Every 3 frames (~50ms)   | 6 bytes     |
| N     | Every N frames           | N × 2 bytes |

### Multi-Frame Input

```
Delay 2 player sends: [0x12][0x00][0xAA][0xBB][0xCC][0xDD]
                                   ├─Frame N─┤ ├─Frame N+1┤
```

Server splits this into individual 2-byte frames.

---

## Frame Synchronization

### Rule

**The server cannot distribute frame N until ALL players have provided input for frame N.**

### Example: 2 Players, Different Delays

```
Setup:
  P0: delay=1 (sends every frame)
  P1: delay=2 (sends every 2 frames)

Timeline:

Time 0ms:
  P0 sends frame 1
  P1 sends frames 1-2
  → Server distributes frame 1: [P0_F1][P1_F1]

Time 16ms:
  P0 sends frame 2
  P1 waits (already sent frames 1-2)
  → Server distributes frame 2: [P0_F2][P1_F2]

Time 33ms:
  P0 sends frame 3
  P1 sends frames 3-4
  → Server distributes frame 3: [P0_F3][P1_F3]
```

### Blocking

```
P0: Frame 5 ✓
P1: Frame 5 ✓
P2: Frame 5 ✗ (missing)

→ Server waits
→ No distribution until P2's input arrives
```

---

## Preemptive Padding

### Formula

```
padding_frames = player_delay - minimum_delay_in_game
```

### Initialization

At game start, slower players' input queues are pre-filled with `[0x00, 0x00]` frames.

**Example: P0 (delay=1), P1 (delay=2), P2 (delay=3)**

```
Initial state:
  P0 queue: []                    (fastest, no padding)
  P1 queue: [[00 00]]             (1 frame padding)
  P2 queue: [[00 00][00 00]]      (2 frames padding)

After first inputs:
  P0 sends [AA BB]
  P1 sends [CC DD][EE FF]
  P2 sends [11 22][33 44][55 66]

Queues:
  P0: [[AA BB]]
  P1: [[00 00][CC DD][EE FF]]
  P2: [[00 00][00 00][11 22][33 44][55 66]]

Frame 1 distribution: [AA BB][00 00][00 00]
```

---

## Frame Distribution

### Output Schedule

Each player receives combined data at their delay rate:

```
Time 0ms:
  Frame 1 ready
  → P0 (delay=1): [Frame_1]
  → P1 (delay=2): waits

Time 16ms:
  Frame 2 ready
  → P0: [Frame_2]
  → P1: [Frame_1][Frame_2]

Time 33ms:
  Frame 3 ready
  → P0: [Frame_3]
  → P1: waits

Time 50ms:
  Frame 4 ready
  → P0: [Frame_4]
  → P1: [Frame_3][Frame_4]
```

### Frame Interleaving

Inputs must be **interleaved by frame**, not concatenated by player.

**WRONG:**

```
P0: [01 00][02 00]
P1: [AA 00][BB 00]

Combined: [01 00][02 00][AA 00][BB 00]  ✗
          └──All P0──┘ └──All P1──┘
```

**CORRECT:**

```
P0: [01 00][02 00]
P1: [AA 00][BB 00]

Combined: [01 00][AA 00][02 00][BB 00]  ✓
          └─Frame 1──┘ └─Frame 2──┘
```

**Algorithm:**

```
FOR each frame F in (0..frame_count):
    FOR each player P in (0..player_count):
        Append player[P].frame[F] to combined_data
```

**Example (3 players, 2 frames):**

```
Input:
  P0: [A1 A2][A3 A4]
  P1: [B1 B2][B3 B4]
  P2: [C1 C2][C3 C4]

Output:
  [A1 A2][B1 B2][C1 C2][A3 A4][B3 B4][C3 C4]
   └────Frame 0──────┘ └────Frame 1──────┘
```

---

## Sequence Diagrams

### Normal Operation

```
Client 0 (delay=1)          Server               Client 1 (delay=1)
      |                       |                         |
      | GD [01 00]           |                         |
      |--------------------->|                         |
      |                       | GD [02 00]             |
      |                       |<------------------------|
      |                       |                         |
      |                   [Combine]                     |
      |                       |                         |
      | GD [01 00 02 00]     |                         |
      |<---------------------|                         |
      |                       | GD [01 00 02 00]       |
      |                       |------------------------>|
```

### Cache Hit

```
Client 0                    Server               Client 1
      |                       |                         |
      | GD [AA BB]           |                         |
      |--------------------->|                         |
      |                       | GD [CC DD]             |
      |                       |<------------------------|
      | GD [AA BB CC DD] (cache pos 0)                 |
      |<------------------------------------------------|
      |                       |                         |
      | GC(0) [AA BB]        |                         |
      |--------------------->|                         |
      |                       | GC(0) [CC DD]          |
      |                       |<------------------------|
      | GC(0) [AA BB CC DD]  |                         |
      |<------------------------------------------------|
```

### Different Delays

```
Client 0 (delay=1)          Server               Client 1 (delay=2)
      |                       |                         |
      | GD [01 00]           |                         |
      |--------------------->|                         |
      |                       | GD [AA BB CC DD]       |
      |                       |<------------------------|
      |                       |                         |
      | GD [01 00 AA BB]     |                         |
      |<---------------------|                         |
      | GD [02 00]           |                         |
      |--------------------->|                         |
      | GD [02 00 CC DD]     |                         |
      |<---------------------|                         |
      |                       | GD [01 00 AA BB]       |
      |                       |      [02 00 CC DD]     |
      |                       |------------------------>|
```

### Game Cache Creates New Combination

```
Frame 1: P0=[AA BB], P1=[CC DD] → [AA BB CC DD] (cache pos 0)
Frame 2: P0 sends GC(0) [AA BB], P1=[EE FF] → [AA BB EE FF] (NEW)

Client 0                    Server               Client 1
      |                       |                         |
      | GD [AA BB]           |                         |
      |--------------------->|                         |
      |                       | GD [CC DD]             |
      |                       |<------------------------|
      | GD [AA BB CC DD]     |                         |
      |<---------------------|                         |
      |                       | GD [AA BB CC DD]       |
      |                       |------------------------>|
      |                       |                         |
      | GC(0) [AA BB]        |                         |
      |--------------------->|                         |
      |                       | GD [EE FF]             |
      |                       |<------------------------|
      | GD [AA BB EE FF]     |                         |
      |<---------------------|                         |
      |                       | GD [AA BB EE FF]       |
      |                       |------------------------>|
```

---
