## Table of Contents

- **[Introduction & Software](#introduction--software)**
- **[Emulators](#emulators)**
- **[List of Commands](#list-of-commands)**
- **[FAQs](#faqs)**
- **[Kaillera Network Protocol](#kaillera-network-protocol)**
- **[Master Servers List (Backup)](#master-servers-list-backup)**

### Websites:

- [EmuLinker](https://EmuLinker.org)
- [Kaillera Reborn](https://kaillerareborn.github.io)

## Kaillera Overview

**Kaillera** is middleware software allowing users to play video games online through emulators. It consists of a client (usually embedded in the emulator) and a server (a standalone application).

### Key Features

- Small, fast, and efficient C++ code
- UDP for low latency
- Intelligent networking cache
- Multi-platform support
- Low to no lag for users with good ping
- Works through firewalls
- Supports LAN/WAN connections

## Delay Frame Table

| Ping    | LAN (60 packets/s) | Excellent (30 packets/s) | Good (20 packets/s) | Average (15 packets/s) | Low (12 packets/s) | Bad (10 packets/s) |
| ------- | ------------------ | ------------------------ | ------------------- | ---------------------- | ------------------ | ------------------ |
| 0–16    | 1 frame            | 3 frames                 | 5 frames            | 7 frames               | 9 frames           | 11 frames          |
| 17–33   | 2 frames           | 5 frames                 | 8 frames            | 11 frames              | 14 frames          | 17 frames          |
| 34–49   | 3 frames           | 7 frames                 | 11 frames           | 15 frames              | 19 frames          | -                  |
| 50–66   | 4 frames           | 9 frames                 | 14 frames           | -                      | -                  | -                  |
| 67–83   | 5 frames           | 11 frames                | -                   | -                      | -                  | -                  |
| 84–99   | 6 frames           | 13 frames                | -                   | -                      | -                  | -                  |
| 100–116 | 7 frames           | -                        | -                   | -                      | -                  | -                  |
| 117–133 | 8 frames           | -                        | -                   | -                      | -                  | -                  |
| 134–149 | 9 frames           | -                        | -                   | -                      | -                  | -                  |
| 150–166 | 10 frames          | -                        | -                   | -                      | -                  | -                  |
| 167–183 | 11 frames          | -                        | -                   | -                      | -                  | -                  |
| 184–199 | 12 frames          | -                        | -                   | -                      | -                  | -                  |

**Note:** A ping below 100 ms (less than a 10-frame delay) is preferable for smoother gameplay.

## Kaillera FAQ

**Why is my game laggy?**

- Lag (network latency) is due to the distance and speed limitations, even with ideal internet conditions. Signal speed, routing, and other factors contribute to lag.

**What is desync?**

- Desync occurs when players' games get out of sync due to data discrepancies. Kaillera only sends keystrokes, not entire game states, which can lead to occasional mismatches.

**What is packet loss?**

- Packet loss is common in UDP-based systems like Kaillera. If a packet is lost, it won’t be re-sent, possibly causing lag or unresponsive gameplay.

**What is choppiness?**

- Choppiness happens when packets are delayed or lost, leading to missing game moves or intermittent lag.

**What is the “Connection” setting?**

- The setting determines packet frequency per second. “Good” (20 packets per second) is a recommended balanced setting for most connections.

**Help! My game goes out of sync!**

- Desyncs are caused by differences in clients’ state due to packet loss or delayed inputs. Scores and player movements can be indicators of desync.

### Kaillera P2P Client FAQ (Partial Backup)

- **What is this?** A Kaillera client for direct connections with lower response delays.
- **IP Issues:** For direct connections, use your external IP (not `127.0.0.1`).
- **Low Ping Advantage:** Both endpoints share the same ping; no advantage for low-ping users.
- **Host Configuration:** Host should forward ports, usually `27886/UDP`.

## Kaillera Network Protocol

Here's the packet format organized in Markdown without a table format for easier readability and maintenance:

### Packet Format Overview

- **Format**: Multi-byte, little-endian (`1st_Byte * 256 + 2nd_Byte`)

**Packet Structure**:

- **Initial Byte**:
  - `1 Byte` - Number of messages in the packet (typically `n-3` during gameplay; may increase as needed).

**Message Header**:

- `2 Bytes` - Message sequence number (incremented with each message).
- `2 Bytes` - Length of the message, including type and data.
- `1 Byte` - Message type (e.g., `0x03` for User Login).

**Message Body**:

- **Data**: Content varies by message type.

**Example**:

```plaintext
(1B), 2B, 2B, 1B, DataA [Repeats] 2B, 2B, 1B, DataB [Repeats] 2B, 2B, 1B, DataC, etc.
```

### Message Types

> **Note**: All multi-byte fields are in little-endian format. Strings are null-terminated (`\0`); empty strings are simply `\0`.

#### 0x01 - User Quit

- **Client to Server**:
  - Empty String (`00`)
  - `2B`: `0xFF`
  - NB: Message
- **Server to Client**:
  - NB: Username
  - `2B`: UserID
  - NB: Message

#### 0x02 - User Joined

- **Server to Client**:
  - NB: Username
  - `2B`: UserID
  - `4B`: Ping
  - `1B`: Connection Type

#### 0x03 - User Login Information

- **Client to Server**:
  - NB: Username
  - NB: Emulator Name
  - `1B`: Connection Type

#### 0x04 - Server Status

- **Server to Client**:
  - NB: Empty String (`00`)
  - `4B`: Users Count
  - `4B`: Games Count
  - NB: Users List
  - NB: Username
  - `4B`: Ping
  - `1B`: Player Status
  - `2B`: UserID
  - `1B`: Connection Type
  - NB: Games List

#### 0x05 - Server to Client ACK

- **Server to Client**:
  - NB: Empty String (`00`)
  - `4B`: `00`, `4B`: `01`, `4B`: `02`, `4B`: `03`

#### 0x06 - Client to Server ACK

- **Client to Server**:
  - Same as Server ACK

#### 0x07 - Global Chat

- **Client to Server**:
  - Empty String
  - NB: Message
- **Server to Client**:
  - NB: Username
  - NB: Message

#### 0x08 - Game Chat

- **Client to Server**:
  - Empty String
  - NB: Message
- **Server to Client**:
  - NB: Username
  - NB: Message

#### 0x09 - Client Keep Alive

- **Client to Server**:
  - NB: Empty String (`00`)

#### 0x0A - Create Game

- **Client to Server**:
  - Empty String (`00`)
  - Game Name
  - Empty String (`00`)
  - `4B`: `0xFF`
- **Server to Client**:
  - NB: Username
  - Game Name
  - Emulator Name
  - `4B`: GameID

#### 0x0B - Quit Game

- **Client to Server**:
  - Empty String
  - `2B`: `0xFF`
- **Server to Client**:
  - NB: Username
  - `2B`: UserID

#### 0x0C - Join Game

- **Client to Server**:
  - Empty String
  - `4B`: GameID
  - Empty String
  - `4B`: `0x00`
  - `2B`: `0xFF`
  - Connection Type(not available; dummy data)
- **Server to Client**:
  - Empty String
  - `4B`: Game Pointer
  - Username
  - Ping
  - UserID
  - Connection Type

#### 0x0D - Player Information

- **Server to Client**:
  - Empty String
  - `4B`: User Count
  - Username
  - Ping
  - UserID
  - Connection Type

#### 0x0E - Update Game Status

- **Server to Client**:
  - Empty String
  - `4B`: GameID
  - Game Status
  - Players in Room
  - Max Players

#### 0x0F - Kick User from Game

- **Client to Server**:
  - Empty String
  - `2B`: UserID

#### 0x10 - Close Game

- **Server to Client**:
  - Empty String
  - `4B`: GameID

#### 0x11 - Start Game

- **Client to Server**:
  - Empty String
  - `2B`: `0xFF`
  - `1B`: `0xFF`
  - `1B`: `0xFF`
- **Server to Client**:
  - Empty String
  - `2B`: Frame Delay(eg. (connectionType * (frameDelay + 1) <-Block on that frame
  - `1B`: Player Number(eg. if you're player 1 or 2...)
  - `1B`: Total Players

#### 0x12 - Game Data

- **Client to Server**:
  - Empty String
  - `2B`: Game Data Length
  - Game Data
- **Server to Client**:
  - Same as Client

#### 0x13 - Game Cache

- **Client to Server**:
  - Empty String
  - Cache Position
- **Server to Client**:
  - Same as Client

#### 0x14 - Drop Game

- **Client to Server**:
  - Empty String
  - `1B`: `0x00`
- **Server to Client**:
  - NB: Username
  - Player Number (who dropped)

#### 0x15 - Ready to Play Signal

- **Client to Server**:
  - Empty String
- **Server to Client**:
  - Empty String

#### 0x16 - Connection Rejected

- **Server to Client**:
  - NB: Username
  - `2B`: UserID
  - NB: Message

#### 0x17 - Server Information Message

- **Server to Client**:
  - NB: "Server\0"
  - NB: Message

### Connection Types

- `6`: Bad
- `5`: Low
- `4`: Average
- `3`: Good
- `2`: Excellent
- `1`: LAN

**Status Codes**:

- **Game Status**:

  - `0`: Waiting
  - `1`: Playing
  - `2`: Netsync

- **Player Status**:
  - `0`: Playing
  - `1`: Idle

## Scenario

### Login State

- **Client**: `HELLO0.83`
- **Server**: Port notification `HELLOD00D#\0` (where `#` is the new port number, e.g., `HELLOD00D7159`)
  - If the server is full: `TOO\0`
- **Client**: Sends **User Login Information** `[0x03]`
- **Server**: Sends **Server to Client ACK** `[0x05]`
- **Client**: Sends **Client to Server ACK** `[0x06]`
  - This **ACK** exchange (Client and Server alternating between `0x05` and `0x06`) is typically repeated 4 times to calculate the client’s ping. Clients respond to Server ACKs.
- **Server**: Sends **Server Status** `[0x04]`
- **Server**: Sends **User Joined** `[0x02]`
- **Server**: Sends **Server Information Message** `[0x17]`

### Global Chat State

- **Client**: Sends **Global Chat Request** `[0x07]`
- **Server**: Sends **Global Chat Notification** `[0x07]`

### Game Chat State

- **Client**: Sends **Game Chat Request** `[0x08]`
- **Server**: Sends **Game Chat Notification** `[0x08]`

### Create Game State

- **Client**: Sends **Create Game Request** `[0x0A]`
- **Server**: Sends **Create Game Notification** `[0x0A]`
- **Server**: Sends **Update Game Status** `[0x0E]`
- **Server**: Sends **Player Information** `[0x0D]`
- **Server**: Sends **Join Game Notification** `[0x0C]`
- **Server**: Sends **Server Information Message** `[0x17]`

### Join Game State

- **Client**: Sends **Join Game Request** `[0x0C]`
- **Server**: Sends **Update Game Status** `[0x0E]`
- **Server**: Sends **Player Information** `[0x0D]`
- **Server**: Sends **Join Game Notification** `[0x0C]`

### Quit Game State

- **Client**: Sends **Quit Game Request** `[0x0B]`
- **Server**: Sends **Update Game Status** `[0x0E]`
- **Server**: Sends **Quit Game Notification** `[0x0B]`

### Close Game State

- **Client**: Sends **Quit Game Request** `[0x0B]`
- **Server**: Sends **Close Game Notification** `[0x10]`
- **Server**: Sends **Quit Game Notification** `[0x0B]`

### Start Game State

- **Client**: Sends **Start Game Request** `[0x11]`
- **Server**: Sends **Update Game Status** `[0x0E]`
- **Server**: Sends **Start Game Notification** `[0x11]`
- **Client**: Enters **Netsync Mode** and waits for all players to send **Ready to Play Signal** `[0x15]`
- **Server**: Sends **Update Game Status** `[0x0E]`
- **Server**: Enters **Playing Mode** after receiving **Ready to Play Signal Notification** `[0x15]` from all players
- **Client(s)**: Exchange data using **Game Data Send** `[0x12]` or **Game Cache Send** `[0x13]`
- **Server**: Sends data accordingly using **Game Data Notify** `[0x12]` or **Game Cache Notify** `[0x13]`

### Drop Game State

- **Client**: Sends **Drop Game Request** `[0x14]`
- **Server**: Sends **Update Game Status** `[0x0E]`
- **Server**: Sends **Drop Game Notification** `[0x14]`

### Kick Player State

- **Client**: Sends **Kick Request** `[0x0F]`
- **Server**: Sends **Quit Game Notification** `[0x0B]`
- **Server**: Sends **Update Game Status** `[0x0E]`

### User Quit State

- **Client**: Sends **User Quit Request** `[0x01]`
- **Server**: Sends **User Quit Notification** `[0x01]`

## Game Data/Game Cache

### 0x12 = Game Data

#### Client Request

- **NB**: Empty String `[00]`
- **2B**: Length of Game Data
- **NB**: Game Data

_Example:_

Suppose we are using **MAME32K 0.64**, which uses **2 bytes per input**. If the **Connection Type** is **3 (Good)**, then:

- **Bytes per player's input**: `Connection Type * 2 bytes`
- **Calculation**: `3 * 2 bytes = 6 bytes`

So, for one player's input, the Game Data length is **6 bytes**.

#### Server Notification

- **NB**: Empty String `[00]`
- **2B**: Length of Game Data
- **NB**: Game Data

_Example:_

If both players are on **Connection Type 3 (Good)** and there are **2 players**, then:

- **Bytes per player**: `3 * 2 bytes = 6 bytes`
- **Total size**: `6 bytes/player * 2 players = 12 bytes`

Therefore, the total size of the incoming data should be **12 bytes**.

---

### 0x13 = Game Cache

#### Client Request

- **NB**: Empty String `[00]`
- **1B**: Cache Position

_256 slots [0 to 255]. Oldest to Newest. When the cache is full:_

- Add new entry at position **255**
- Shift all old entries down, knocking off the oldest
- **Search cache for matching data before you send**
  - If found, send that cache position
  - Otherwise, issue a **Game Data Send [0x12]**

_Example:_

- You have new game data to send.
- You search the cache and find a match at position **42**.
- You send the cache position `[00][2A]` (since `2A` in hex is `42`).

_If no match is found:_

- Send a **Game Data Send [0x12]**.
- Add the new data to position **255** in the cache.
- Shift existing entries down by one position.

#### Server Notification

- **NB**: Empty String `[00]`
- **1B**: Cache Position

_Uses the same cache procedure as above._

_Example:_

- The server notifies you with cache position **100**.
- You retrieve the data from your cache at position **100**.
- If the data is not found, you handle it accordingly (e.g., request data).

---

### Connection Types and Data Size Calculation

**Connection Types:**

- **1**: Poor
- **2**: Average
- **3**: Good

**Bytes per Input:**

- **Calculation**: `Connection Type * 2 bytes`

_Examples:_

- **Connection Type 1 (Poor)**: `1 * 2 bytes = 2 bytes`
- **Connection Type 2 (Average)**: `2 * 2 bytes = 4 bytes`
- **Connection Type 3 (Good)**: `3 * 2 bytes = 6 bytes`

---

### Expanded Example for Multiple Players

Suppose we have **4 players** with the following connection types:

- **Player 1**: Connection Type **3 (Good)**
- **Player 2**: Connection Type **2 (Average)**
- **Player 3**: Connection Type **3 (Good)**
- **Player 4**: Connection Type **1 (Poor)**

**Calculate Bytes per Player:**

- **Player 1**: `3 * 2 bytes = 6 bytes`
- **Player 2**: `2 * 2 bytes = 4 bytes`
- **Player 3**: `3 * 2 bytes = 6 bytes`
- **Player 4**: `1 * 2 bytes = 2 bytes`

**Total Game Data Size:**

```
Total size = 6 + 4 + 6 + 2 = 18 bytes
```

---

### Notes on Cache Management

- **Cache Size**: 256 entries (positions **0** to **255**)
- **When Full**:
  - Add new data at position **255**
  - Shift existing entries down
  - Remove entry at position **0**

_Cache Update Example:_

1. **Before Update**:

   | Position | Data     |
   | -------- | -------- |
   | 255      | Data A   |
   | 254      | Data B   |
   | 253      | Data C   |
   | ...      | ...      |
   | 0        | Data Old |

2. **After Adding New Data (Data New)**:

   - **Data New** at position **255**
   - **Data A** moves to **254**
   - **Data B** moves to **253**
   - **Data Old** is removed

_Cache Search Example:_

- **Scenario**: Sending data
- **Cache Hit**: Data found at position **75**
  - Send `[00][4B]` (hex for 75)
- **Cache Miss**: Data not found
  - Send **Game Data [0x12]**
  - Add data to cache at **255**

---

### Summary

- **Game Data [0x12]** is used to send actual game inputs.
- **Game Cache [0x13]** optimizes data transfer by referencing cached inputs.
- **Connection Type** affects the size of input data per player.
- Proper cache management is crucial for efficient network communication.

---
