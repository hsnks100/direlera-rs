// Game Data/Game Cache synchronization module with predefined delays and independent send queues
// Handles KAILLERA protocol's frame synchronization with player-specific delays

use std::collections::VecDeque;

/// Client input message type
#[derive(Debug, Clone, PartialEq)]
pub enum ClientInput {
    /// Game Data: contains the actual input bytes (always 2 bytes per frame)
    GameData(Vec<u8>),
    /// Game Cache: references a position in the client's cache
    GameCache(u8),
}

/// Server response message type
#[derive(Debug, Clone, PartialEq)]
pub enum ServerResponse {
    /// Game Data: contains the full combined input bytes
    GameData(Vec<u8>),
    /// Game Cache: references a position in the server's cache
    GameCache(u8),
}

/// Output action that should be taken for a specific player
#[derive(Debug, Clone, PartialEq)]
pub struct PlayerOutput {
    pub player_id: usize,
    pub response: ServerResponse,
}

/// FIFO cache with 256 slots for storing input data
#[derive(Debug, Clone)]
struct InputCache {
    slots: Vec<Vec<u8>>,
}

impl InputCache {
    fn new() -> Self {
        Self {
            slots: Vec::with_capacity(256),
        }
    }

    /// Find data in cache, returning the position (0-255) if found
    /// Searches from newest (end) to oldest (start)
    fn find(&self, data: &[u8]) -> Option<u8> {
        self.slots
            .iter()
            .enumerate()
            .rev()
            .find(|(_, cached_data)| cached_data.as_slice() == data)
            .map(|(index, _)| index as u8)
    }

    /// Update cache by shifting all elements and adding new data at position 255
    fn update(&mut self, data: Vec<u8>) {
        // If cache is full, remove the oldest entry
        if self.slots.len() >= 256 {
            self.slots.remove(0);
        }
        // Add new data at the end (position 255 when full)
        self.slots.push(data);
    }

    /// Get data at a specific position
    fn get(&self, position: u8) -> Option<&[u8]> {
        self.slots.get(position as usize).map(|v| v.as_slice())
    }
}

/// Master input queue for a single player
#[derive(Debug, Clone)]
struct MasterInputQueue {
    /// Queue of raw input data (2 bytes per frame)
    queue: VecDeque<Vec<u8>>,
    /// Client's individual input cache
    client_cache: InputCache,
    /// Expected input size for this player (delay * 2)
    expected_input_size: usize,
}

impl MasterInputQueue {
    fn new(delay: usize) -> Self {
        Self {
            queue: VecDeque::new(),
            client_cache: InputCache::new(),
            expected_input_size: delay * 2,
        }
    }

    /// Add input to the master queue (splits multi-frame input into 2-byte chunks)
    fn push(&mut self, input: Vec<u8>) {
        // Split input into 2-byte frames
        for chunk in input.chunks(2) {
            if chunk.len() == 2 {
                self.queue.push_back(chunk.to_vec());
            }
        }
    }

    /// Get the current size of the queue
    fn len(&self) -> usize {
        self.queue.len()
    }
}

/// Per-player send queue (independent transmission buffer)
#[derive(Debug, Clone)]
struct PlayerSendQueue {
    /// Buffers for each player's input data
    /// player_buffers[i] contains the input data from player i
    player_buffers: Vec<VecDeque<u8>>,
    /// Delay value for this player (1 = 1/60s, 2 = 2/60s, etc.)
    delay: usize,
    /// Bytes per frame (always 2 for standard input)
    bytes_per_frame: usize,
}

impl PlayerSendQueue {
    fn new(player_count: usize, delay: usize) -> Self {
        Self {
            player_buffers: vec![VecDeque::new(); player_count],
            delay,
            bytes_per_frame: 2,
        }
    }

    /// Add input data from a specific player to this send queue
    fn push_input(&mut self, player_id: usize, data: &[u8]) {
        self.player_buffers[player_id].extend(data);
    }

    /// Check if we have enough data to send
    /// For delay N, we need N * bytes_per_frame from each player
    fn can_send(&self) -> bool {
        let required_bytes = self.delay * self.bytes_per_frame;
        self.player_buffers
            .iter()
            .all(|buffer| buffer.len() >= required_bytes)
    }

    /// Extract the required amount of data from all player buffers
    /// Returns combined data if ready, None otherwise
    fn extract_combined(&mut self) -> Option<Vec<u8>> {
        if !self.can_send() {
            return None;
        }

        let bytes_to_extract = self.delay * self.bytes_per_frame;
        let mut combined = Vec::with_capacity(bytes_to_extract * self.player_buffers.len());

        for buffer in &mut self.player_buffers {
            for _ in 0..bytes_to_extract {
                if let Some(byte) = buffer.pop_front() {
                    combined.push(byte);
                }
            }
        }

        Some(combined)
    }
}

/// Game synchronization manager with delay-based architecture
#[derive(Debug, Clone)]
pub struct GameSyncManager {
    /// Number of players in the game
    player_count: usize,
    /// Master input queues for each player
    master_queues: Vec<MasterInputQueue>,
    /// Independent send queues for each player
    send_queues: Vec<PlayerSendQueue>,
    /// Player delays (in frames)
    player_delays: Vec<usize>,
    /// Per-player output cache (what each player has received)
    player_output_caches: Vec<InputCache>,
    /// Minimum delay among all players
    min_delay: usize,
}

impl GameSyncManager {
    /// Create a new game sync manager with player delays
    /// delays: array of delay values for each player (1 = 1/60s, 2 = 2/60s, etc.)
    pub fn new(delays: Vec<usize>) -> Self {
        let player_count = delays.len();
        assert!(player_count >= 1, "At least 1 player required");

        for (i, &delay) in delays.iter().enumerate() {
            assert!(delay > 0, "Player {} delay must be positive", i);
        }

        let min_delay = *delays.iter().min().unwrap();

        let mut manager = Self {
            player_count,
            master_queues: delays
                .iter()
                .map(|&delay| MasterInputQueue::new(delay))
                .collect(),
            send_queues: delays
                .iter()
                .map(|&delay| PlayerSendQueue::new(player_count, delay))
                .collect(),
            player_delays: delays.clone(),
            player_output_caches: (0..player_count).map(|_| InputCache::new()).collect(),
            min_delay,
        };

        // Apply preemptive padding
        manager.apply_preemptive_padding();

        manager
    }

    /// Apply preemptive padding at game start
    /// Players with higher delay get empty inputs [00 00] in their master queue
    fn apply_preemptive_padding(&mut self) {
        let empty_input = vec![0x00, 0x00];

        for player_id in 0..self.player_count {
            let padding_frames = self.player_delays[player_id] - self.min_delay;
            for _ in 0..padding_frames {
                self.master_queues[player_id].push(empty_input.clone());
            }
        }

        // Note: Padding will be distributed when first real input arrives
        // This is intentional - we don't distribute until all players have data
    }

    /// Distribute new inputs from master queues to all send queues
    /// Only distributes when all players have at least one input ready
    fn distribute_to_send_queues(&mut self) {
        // Find the minimum master queue length
        let min_master_len = self
            .master_queues
            .iter()
            .map(|q| q.len())
            .min()
            .unwrap_or(0);

        if min_master_len == 0 {
            return;
        }

        // Distribute one frame at a time
        for _ in 0..min_master_len {
            // For each send queue, copy one frame from all master queues
            for send_queue in &mut self.send_queues {
                for player_id in 0..self.player_count {
                    if let Some(data) = self.master_queues[player_id].queue.front() {
                        send_queue.push_input(player_id, data);
                    }
                }
            }

            // Remove the processed frame from all master queues
            for master_queue in &mut self.master_queues {
                master_queue.queue.pop_front();
            }
        }
    }

    /// Process a client's input
    /// Returns a list of outputs to send to specific players
    pub fn process_client_input(
        &mut self,
        player_id: usize,
        input: ClientInput,
    ) -> Vec<PlayerOutput> {
        assert!(player_id < self.player_count, "Invalid player_id");

        let expected_size = self.master_queues[player_id].expected_input_size;

        // Resolve the actual input data (delay * 2 bytes)
        let input_data = match input {
            ClientInput::GameData(data) => {
                assert_eq!(
                    data.len(),
                    expected_size,
                    "Player {} input data must be {} bytes (delay {} * 2), got {}",
                    player_id,
                    expected_size,
                    self.player_delays[player_id],
                    data.len()
                );
                // Update the client's cache
                self.master_queues[player_id]
                    .client_cache
                    .update(data.clone());
                data
            }
            ClientInput::GameCache(position) => {
                // Restore data from the client's cache
                self.master_queues[player_id]
                    .client_cache
                    .get(position)
                    .expect("Cache position not found")
                    .to_vec()
            }
        };

        // Add input to master queue (will be split into 2-byte frames)
        self.master_queues[player_id].push(input_data);

        // Distribute to send queues
        self.distribute_to_send_queues();

        // Try to send data to players who are ready
        self.try_send_to_all_players()
    }

    /// Try to send data to all players who have enough data in their send queues
    fn try_send_to_all_players(&mut self) -> Vec<PlayerOutput> {
        let mut outputs = Vec::new();

        // Keep trying until no player has data ready
        loop {
            let mut any_ready = false;

            // Check which players have data ready and extract from all of them
            let mut extractions: Vec<(usize, Vec<u8>)> = Vec::new();
            for player_id in 0..self.player_count {
                if let Some(combined_data) = self.send_queues[player_id].extract_combined() {
                    extractions.push((player_id, combined_data));
                    any_ready = true;
                }
            }

            if !any_ready {
                break;
            }

            // Process all extractions and generate responses
            // Each player checks their own output cache (what they have received before)
            for (player_id, combined_data) in extractions {
                let player_cache = &mut self.player_output_caches[player_id];

                let response = if let Some(cache_position) = player_cache.find(&combined_data) {
                    // This player has received this data before
                    ServerResponse::GameCache(cache_position)
                } else {
                    // New data for this player - send full data and update their cache
                    player_cache.update(combined_data.clone());
                    ServerResponse::GameData(combined_data)
                };

                outputs.push(PlayerOutput {
                    player_id,
                    response,
                });
            }
        }

        outputs
    }

    /// Get the delay value for a specific player
    pub fn get_player_delay(&self, player_id: usize) -> usize {
        assert!(player_id < self.player_count, "Invalid player_id");
        self.player_delays[player_id]
    }

    /// Get the number of players
    pub fn player_count(&self) -> usize {
        self.player_count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_equal_delays() {
        // Both players have delay 1 (same speed)
        let mut manager = GameSyncManager::new(vec![1, 1]);

        // Frame 1: P0 sends input
        let outputs = manager.process_client_input(0, ClientInput::GameData(vec![0x01, 0x02]));
        assert_eq!(outputs.len(), 0); // Not ready yet (need P1's input)

        // Frame 1: P1 sends input
        let outputs = manager.process_client_input(1, ClientInput::GameData(vec![0x03, 0x04]));
        assert_eq!(outputs.len(), 2); // Both players get data

        // Check Frame 1 outputs directly
        assert_eq!(outputs[0].player_id, 0);
        assert_eq!(outputs[1].player_id, 1);

        // Both outputs should be GameData (first time each player receives this data)
        match &outputs[0].response {
            ServerResponse::GameData(data) => {
                assert_eq!(data, &vec![0x01, 0x02, 0x03, 0x04]);
            }
            ServerResponse::GameCache(_) => {
                panic!("P0's first output should be GameData, not GameCache");
            }
        }

        match &outputs[1].response {
            ServerResponse::GameData(data) => {
                assert_eq!(data, &vec![0x01, 0x02, 0x03, 0x04]);
            }
            ServerResponse::GameCache(_) => {
                panic!("P1's first output should be GameData (P1 has no cache yet)");
            }
        }

        // Frame 2: Both send same inputs via cache
        manager.process_client_input(0, ClientInput::GameCache(0)); // [01 02]
        let outputs = manager.process_client_input(1, ClientInput::GameCache(0)); // [03 04]

        // Frame 2: Combined should be same as Frame 1, so GameCache
        assert_eq!(outputs.len(), 2);
        assert_eq!(outputs[0].player_id, 0);
        assert_eq!(outputs[1].player_id, 1);

        // Both should get GameCache (data already in server cache)
        assert!(
            matches!(outputs[0].response, ServerResponse::GameCache(_)),
            "P0 should get GameCache"
        );
        assert!(
            matches!(outputs[1].response, ServerResponse::GameCache(_)),
            "P1 should get GameCache"
        );
    }

    #[test]
    fn test_different_delays() {
        // P0: 1/60, P1: 2/60
        let mut manager = GameSyncManager::new(vec![1, 2]);

        // Initial state: P1 should have 1 frame of padding [00 00]
        // P0 master queue: []
        // P1 master queue: [[00 00]]

        // Step 1: P0 sends first input [01 00]
        let outputs = manager.process_client_input(0, ClientInput::GameData(vec![0x01, 0x00]));
        // P0 master: [01 00], P1 master: [00 00]
        // Distribute: both send queues get P0=[01 00], P1=[00 00]
        // P0 needs 2 bytes per player = ready! (2+2=4 bytes)
        // P1 needs 4 bytes per player = not ready (2+2=4, but needs 4+4=8)
        assert_eq!(outputs.len(), 1);
        assert_eq!(outputs[0].player_id, 0);
        assert_eq!(
            outputs[0].response,
            ServerResponse::GameData(vec![0x01, 0x00, 0x00, 0x00])
        );

        // Step 2: P0 sends second input [02 00]
        let outputs = manager.process_client_input(0, ClientInput::GameData(vec![0x02, 0x00]));
        // P0 master: [02 00], P1 master: [] (padding exhausted)
        // Cannot distribute (P1 has no data)
        // P0 still waiting for P1
        assert_eq!(outputs.len(), 0);

        // Step 3: P0 sends third input [03 00]
        let outputs = manager.process_client_input(0, ClientInput::GameData(vec![0x03, 0x00]));
        // P0 master: [02 00, 03 00], P1 master: []
        // Cannot distribute (P1 has no data)
        assert_eq!(outputs.len(), 0);

        // Step 4: P1 finally sends first input [AA BB CC DD] (delay 2 = 4 bytes)
        let outputs =
            manager.process_client_input(1, ClientInput::GameData(vec![0xAA, 0xBB, 0xCC, 0xDD]));
        // P0 master: [02 00, 03 00], P1 master: [AA BB, CC DD] (split into 2 frames)
        // Distribute 2 frames: P0=[02 00, 03 00], P1=[AA BB, CC DD]
        //
        // P0 send_queue: P0=[02 00, 03 00], P1=[AA BB, CC DD] (8 bytes)
        //   → P0 sends [02 00 AA BB] (frame 1)
        //   → P0 sends [03 00 CC DD] (frame 2)
        //
        // P1 send_queue BEFORE: P0=[01 00], P1=[00 00] (4 bytes, needs 8)
        // P1 send_queue AFTER:  P0=[01 00, 02 00, 03 00], P1=[00 00, AA BB, CC DD] (12 bytes)
        //   → P1 sends [01 00 02 00 00 00 AA BB] (8 bytes)
        //   → P1 has 4 bytes left (not enough for next frame)
        assert!(outputs.len() >= 2, "At least P0 x2 and P1 x1");

        // P0 should get 2 frames, P1 should get 1 frame
        let p0_outputs: Vec<_> = outputs.iter().filter(|o| o.player_id == 0).collect();
        let p1_outputs: Vec<_> = outputs.iter().filter(|o| o.player_id == 1).collect();

        assert_eq!(p0_outputs.len(), 2, "P0 should get 2 frames");
        assert_eq!(p1_outputs.len(), 1, "P1 should get 1 frame");

        // P0's first frame [02 00 AA BB]
        match &p0_outputs[0].response {
            ServerResponse::GameData(data) => {
                assert_eq!(data, &vec![0x02, 0x00, 0xAA, 0xBB]);
            }
            _ => panic!("P0 should get GameData"),
        }

        // P0's second frame [03 00 CC DD]
        match &p0_outputs[1].response {
            ServerResponse::GameData(data) => {
                assert_eq!(data, &vec![0x03, 0x00, 0xCC, 0xDD]);
            }
            _ => panic!("P0 should get GameData"),
        }

        // P1's frame [01 00 02 00 00 00 AA BB]
        match &p1_outputs[0].response {
            ServerResponse::GameData(data) => {
                assert_eq!(data, &vec![0x01, 0x00, 0x02, 0x00, 0x00, 0x00, 0xAA, 0xBB]);
            }
            _ => panic!("P1 should get GameData"),
        }
    }

    #[test]
    fn test_cache_mechanism() {
        // Both players with delay 1
        let mut manager = GameSyncManager::new(vec![1, 1]);

        // Frame 1: Both send [00 00]
        manager.process_client_input(0, ClientInput::GameData(vec![0x00, 0x00]));
        let outputs = manager.process_client_input(1, ClientInput::GameData(vec![0x00, 0x00]));

        assert_eq!(outputs.len(), 2);
        // First time should be GameData
        assert!(matches!(outputs[0].response, ServerResponse::GameData(_)));

        // Frame 2: Both use GameCache(0)
        manager.process_client_input(0, ClientInput::GameCache(0));
        let outputs = manager.process_client_input(1, ClientInput::GameCache(0));

        // Server should send GameCache because combined data is the same
        let has_cache_response = outputs
            .iter()
            .any(|o| matches!(o.response, ServerResponse::GameCache(_)));
        assert!(has_cache_response);
    }

    #[test]
    fn test_three_players() {
        // P1: 1/60, P2: 1/60, P3: 2/60
        let mut manager = GameSyncManager::new(vec![1, 1, 2]);

        // P0 sends
        manager.process_client_input(0, ClientInput::GameData(vec![0x01, 0x00]));
        // P1 sends
        let outputs = manager.process_client_input(1, ClientInput::GameData(vec![0x02, 0x00]));

        // P0 and P1 should get data (they have delay 1)
        // P2 should not get data yet (needs more bytes)
        assert_eq!(outputs.len(), 2);
        assert_eq!(outputs[0].player_id, 0);
        assert_eq!(outputs[1].player_id, 1);

        // Both P0 and P1 get combined data: [01 00 02 00 00 00]
        match &outputs[0].response {
            ServerResponse::GameData(data) => {
                assert_eq!(data, &vec![0x01, 0x00, 0x02, 0x00, 0x00, 0x00]);
            }
            ServerResponse::GameCache(_) => {}
        }
        match &outputs[1].response {
            ServerResponse::GameData(data) => {
                assert_eq!(data, &vec![0x01, 0x00, 0x02, 0x00, 0x00, 0x00]);
            }
            ServerResponse::GameCache(_) => {}
        }
    }

    #[test]
    fn test_preemptive_padding() {
        // P0: 1/60, P1: 3/60 (2 frames difference)
        let mut manager = GameSyncManager::new(vec![1, 3]);

        // After padding, P1 should have 2 frames in master queue (not yet distributed)
        assert_eq!(manager.master_queues[0].len(), 0);
        assert_eq!(
            manager.master_queues[1].len(),
            2,
            "P1 should have 2 padding frames"
        );

        // Verify padding works by sending inputs
        // P0 sends first input - padding will be distributed now
        let outputs = manager.process_client_input(0, ClientInput::GameData(vec![0x01, 0x00]));

        // P0 (delay 1) needs 2 bytes per player = 4 bytes total
        // After distribute: P0 send_queue has [01 00] from P0 and [00 00] from P1's padding
        assert_eq!(outputs.len(), 1, "P0 should get output after first input");
        assert_eq!(outputs[0].player_id, 0);

        match &outputs[0].response {
            ServerResponse::GameData(data) => {
                // Combined: P0's [01 00] + P1's [00 00] from padding
                assert_eq!(data, &vec![0x01, 0x00, 0x00, 0x00]);
            }
            ServerResponse::GameCache(_) => panic!("First output should be GameData"),
        }

        // P0 sends second input
        let outputs = manager.process_client_input(0, ClientInput::GameData(vec![0x02, 0x00]));
        assert_eq!(outputs.len(), 1, "P0 should get second output");
        assert_eq!(outputs[0].player_id, 0);

        match &outputs[0].response {
            ServerResponse::GameData(data) => {
                // Combined: P0's [02 00] + P1's [00 00] from padding
                assert_eq!(data, &vec![0x02, 0x00, 0x00, 0x00]);
            }
            ServerResponse::GameCache(_) => {}
        }
    }

    #[test]
    fn test_gd_gc_pattern_delay_1() {
        // Test pattern: GD GC GC GD GC GC with delay 1
        let mut manager = GameSyncManager::new(vec![1, 1]);

        // Frame 1: GD GD (both send new data)
        manager.process_client_input(0, ClientInput::GameData(vec![0xAA, 0xBB]));
        let outputs = manager.process_client_input(1, ClientInput::GameData(vec![0xCC, 0xDD]));

        assert_eq!(outputs.len(), 2);
        assert_eq!(outputs[0].player_id, 0);
        assert_eq!(outputs[1].player_id, 1);

        // Both should receive GameData (first time)
        match &outputs[0].response {
            ServerResponse::GameData(data) => {
                assert_eq!(data, &vec![0xAA, 0xBB, 0xCC, 0xDD]);
            }
            _ => panic!("Frame 1: P0 should get GameData"),
        }
        match &outputs[1].response {
            ServerResponse::GameData(data) => {
                assert_eq!(data, &vec![0xAA, 0xBB, 0xCC, 0xDD]);
            }
            _ => panic!("Frame 1: P1 should get GameData"),
        }

        // Frame 2: GC GC (both send cached data - same as frame 1)
        manager.process_client_input(0, ClientInput::GameCache(0)); // [AA BB]
        let outputs = manager.process_client_input(1, ClientInput::GameCache(0)); // [CC DD]

        assert_eq!(outputs.len(), 2);
        // Both should receive GameCache (same combined data as frame 1)
        assert!(
            matches!(outputs[0].response, ServerResponse::GameCache(0)),
            "Frame 2: P0 should get GameCache(0)"
        );
        assert!(
            matches!(outputs[1].response, ServerResponse::GameCache(0)),
            "Frame 2: P1 should get GameCache(0)"
        );

        // Frame 3: GC GC (both send cached data - same as frame 1)
        manager.process_client_input(0, ClientInput::GameCache(0)); // [AA BB]
        let outputs = manager.process_client_input(1, ClientInput::GameCache(0)); // [CC DD]

        assert_eq!(outputs.len(), 2);
        // Both should receive GameCache
        assert!(matches!(outputs[0].response, ServerResponse::GameCache(0)));
        assert!(matches!(outputs[1].response, ServerResponse::GameCache(0)));

        // Frame 4: GD GD (both send NEW data)
        manager.process_client_input(0, ClientInput::GameData(vec![0x11, 0x22]));
        let outputs = manager.process_client_input(1, ClientInput::GameData(vec![0x33, 0x44]));

        assert_eq!(outputs.len(), 2);
        // Both should receive GameData (new combined data)
        match &outputs[0].response {
            ServerResponse::GameData(data) => {
                assert_eq!(data, &vec![0x11, 0x22, 0x33, 0x44]);
            }
            _ => panic!("Frame 4: P0 should get GameData"),
        }
        match &outputs[1].response {
            ServerResponse::GameData(data) => {
                assert_eq!(data, &vec![0x11, 0x22, 0x33, 0x44]);
            }
            _ => panic!("Frame 4: P1 should get GameData"),
        }

        // Frame 5: GC GC (both send cached data - same as frame 4)
        manager.process_client_input(0, ClientInput::GameCache(1)); // [11 22]
        let outputs = manager.process_client_input(1, ClientInput::GameCache(1)); // [33 44]

        assert_eq!(outputs.len(), 2);
        // Both should receive GameCache (same combined data as frame 4)
        assert!(
            matches!(outputs[0].response, ServerResponse::GameCache(1)),
            "Frame 5: P0 should get GameCache(1)"
        );
        assert!(
            matches!(outputs[1].response, ServerResponse::GameCache(1)),
            "Frame 5: P1 should get GameCache(1)"
        );

        // Frame 6: GC GC (both send cached data - same as frame 4)
        manager.process_client_input(0, ClientInput::GameCache(1)); // [11 22]
        let outputs = manager.process_client_input(1, ClientInput::GameCache(1)); // [33 44]

        assert_eq!(outputs.len(), 2);
        // Both should receive GameCache
        assert!(matches!(outputs[0].response, ServerResponse::GameCache(1)));
        assert!(matches!(outputs[1].response, ServerResponse::GameCache(1)));
    }

    #[test]
    fn test_gd_gc_pattern_delay_2() {
        // Test pattern: GD GC GD GC with delay 2 (each input = 4 bytes = 2 frames)
        let mut manager = GameSyncManager::new(vec![2, 2]);

        // Frame 1-2: GD GD (both send 4 bytes = 2 frames worth)
        manager.process_client_input(0, ClientInput::GameData(vec![0xAA, 0xBB, 0xAA, 0xBB]));
        let outputs =
            manager.process_client_input(1, ClientInput::GameData(vec![0xCC, 0xDD, 0xCC, 0xDD]));

        // Both should receive data (2 frames accumulated)
        assert_eq!(outputs.len(), 2);

        // Both should receive GameData (first time receiving)
        // Combined: [AA BB AA BB CC DD CC DD] (4 bytes from each player)
        match &outputs[0].response {
            ServerResponse::GameData(data) => {
                assert_eq!(data.len(), 8); // 4 bytes from P0 + 4 bytes from P1
                assert_eq!(data, &vec![0xAA, 0xBB, 0xAA, 0xBB, 0xCC, 0xDD, 0xCC, 0xDD]);
            }
            _ => panic!("P0 should get GameData"),
        }
        match &outputs[1].response {
            ServerResponse::GameData(data) => {
                assert_eq!(data, &vec![0xAA, 0xBB, 0xAA, 0xBB, 0xCC, 0xDD, 0xCC, 0xDD]);
            }
            _ => panic!("P1 should get GameData"),
        }

        // Frame 3-4: GC GC (both send cached 4 bytes - same as frame 1-2)
        manager.process_client_input(0, ClientInput::GameCache(0)); // [AA BB AA BB]
        let outputs = manager.process_client_input(1, ClientInput::GameCache(0)); // [CC DD CC DD]

        // Both should receive GameCache (same combined data as before)
        assert_eq!(outputs.len(), 2);
        assert!(matches!(outputs[0].response, ServerResponse::GameCache(0)));
        assert!(matches!(outputs[1].response, ServerResponse::GameCache(0)));

        // Frame 5-6: GD GD (both send NEW 4 bytes)
        manager.process_client_input(0, ClientInput::GameData(vec![0x11, 0x22, 0x11, 0x22]));
        let outputs =
            manager.process_client_input(1, ClientInput::GameData(vec![0x33, 0x44, 0x33, 0x44]));

        // Both should receive GameData (new combined data)
        assert_eq!(outputs.len(), 2);

        // Combined: [11 22 11 22 33 44 33 44]
        match &outputs[0].response {
            ServerResponse::GameData(data) => {
                assert_eq!(data, &vec![0x11, 0x22, 0x11, 0x22, 0x33, 0x44, 0x33, 0x44]);
            }
            _ => panic!("P0 should get GameData"),
        }
        match &outputs[1].response {
            ServerResponse::GameData(data) => {
                assert_eq!(data, &vec![0x11, 0x22, 0x11, 0x22, 0x33, 0x44, 0x33, 0x44]);
            }
            _ => panic!("P1 should get GameData"),
        }

        // Frame 7-8: GC GC (both send cached data - same as frame 5-6)
        manager.process_client_input(0, ClientInput::GameCache(1)); // [11 22 11 22]
        let outputs = manager.process_client_input(1, ClientInput::GameCache(1)); // [33 44 33 44]

        // Both should receive GameCache (same combined data as frame 5-6)
        assert_eq!(outputs.len(), 2);
        assert!(matches!(outputs[0].response, ServerResponse::GameCache(1)));
        assert!(matches!(outputs[1].response, ServerResponse::GameCache(1)));
    }

    #[test]
    fn test_gd_gc_creates_new_combined() {
        // Test: GD GD -> GD GC should create new combined data
        let mut manager = GameSyncManager::new(vec![1, 1]);

        // Frame 1: GD GD - both send new data
        manager.process_client_input(0, ClientInput::GameData(vec![0x01, 0x02]));
        let outputs = manager.process_client_input(1, ClientInput::GameData(vec![0x03, 0x04]));

        assert_eq!(outputs.len(), 2);
        // Combined: [01 02 03 04] - GameData
        match &outputs[0].response {
            ServerResponse::GameData(data) => {
                assert_eq!(data, &vec![0x01, 0x02, 0x03, 0x04]);
            }
            _ => panic!("Frame 1: P0 should get GameData"),
        }

        // Frame 2: GD GC - P0 sends new data, P1 sends cached data
        manager.process_client_input(0, ClientInput::GameData(vec![0x05, 0x06]));
        let outputs = manager.process_client_input(1, ClientInput::GameCache(0)); // [03 04]

        assert_eq!(outputs.len(), 2);
        // Combined: [05 06 03 04] - This is NEW combined data!
        // Even though P1 used cache, P0's new data makes the combined data new
        match &outputs[0].response {
            ServerResponse::GameData(data) => {
                assert_eq!(
                    data,
                    &vec![0x05, 0x06, 0x03, 0x04],
                    "P0 should get NEW GameData"
                );
            }
            ServerResponse::GameCache(_) => panic!("P0 should get GameData, not GameCache"),
        }
        match &outputs[1].response {
            ServerResponse::GameData(data) => {
                assert_eq!(
                    data,
                    &vec![0x05, 0x06, 0x03, 0x04],
                    "P1 should get NEW GameData"
                );
            }
            ServerResponse::GameCache(_) => panic!("P1 should get GameData, not GameCache"),
        }

        // Frame 3: GD GC - P0 sends different new data, P1 still uses cache
        manager.process_client_input(0, ClientInput::GameData(vec![0x07, 0x08]));
        let outputs = manager.process_client_input(1, ClientInput::GameCache(0)); // [03 04]

        assert_eq!(outputs.len(), 2);
        // Combined: [07 08 03 04] - Another NEW combined data!
        match &outputs[0].response {
            ServerResponse::GameData(data) => {
                assert_eq!(
                    data,
                    &vec![0x07, 0x08, 0x03, 0x04],
                    "P0 should get another NEW GameData"
                );
            }
            _ => panic!("P0 should get GameData"),
        }
        match &outputs[1].response {
            ServerResponse::GameData(data) => {
                assert_eq!(
                    data,
                    &vec![0x07, 0x08, 0x03, 0x04],
                    "P1 should get another NEW GameData"
                );
            }
            _ => panic!("P1 should get GameData"),
        }

        // Frame 4: GC GC - Both use cache from Frame 3
        manager.process_client_input(0, ClientInput::GameCache(2)); // [07 08]
        let outputs = manager.process_client_input(1, ClientInput::GameCache(0)); // [03 04]

        assert_eq!(outputs.len(), 2);
        // Combined: [07 08 03 04] - Same as Frame 3, should be GameCache
        assert!(
            matches!(outputs[0].response, ServerResponse::GameCache(2)),
            "P0 should get GameCache (same as Frame 3)"
        );
        assert!(
            matches!(outputs[1].response, ServerResponse::GameCache(2)),
            "P1 should get GameCache (same as Frame 3)"
        );
    }

    #[test]
    fn test_gc_gc_creates_new_combined() {
        // Test: GC GC can create new combined data if players reference different cache positions
        let mut manager = GameSyncManager::new(vec![1, 1]);

        // Frame 1: P0[01 02] + P1[03 04] → Combined [01 02 03 04]
        manager.process_client_input(0, ClientInput::GameData(vec![0x01, 0x02]));
        let outputs = manager.process_client_input(1, ClientInput::GameData(vec![0x03, 0x04]));
        assert_eq!(outputs.len(), 2);
        match &outputs[0].response {
            ServerResponse::GameData(data) => {
                assert_eq!(data, &vec![0x01, 0x02, 0x03, 0x04]);
            }
            _ => panic!("Frame 1 should be GameData"),
        }

        // Frame 2: P0[01 02] + P1[05 06] → Combined [01 02 05 06]
        manager.process_client_input(0, ClientInput::GameCache(0)); // [01 02]
        let outputs = manager.process_client_input(1, ClientInput::GameData(vec![0x05, 0x06]));
        assert_eq!(outputs.len(), 2);
        match &outputs[0].response {
            ServerResponse::GameData(data) => {
                assert_eq!(data, &vec![0x01, 0x02, 0x05, 0x06]);
            }
            _ => panic!("Frame 2 should be GameData"),
        }

        // Frame 3: P0[07 08] + P1[03 04] → Combined [07 08 03 04]
        manager.process_client_input(0, ClientInput::GameData(vec![0x07, 0x08]));
        let outputs = manager.process_client_input(1, ClientInput::GameCache(0)); // [03 04]
        assert_eq!(outputs.len(), 2);
        match &outputs[0].response {
            ServerResponse::GameData(data) => {
                assert_eq!(data, &vec![0x07, 0x08, 0x03, 0x04]);
            }
            _ => panic!("Frame 3 should be GameData"),
        }

        // Now cache state:
        // P0 client cache: [0]=[01 02], [1]=[07 08]
        // P1 client cache: [0]=[03 04], [1]=[05 06]

        // Frame 4: GC GC - P0[01 02] + P1[05 06] → Combined [01 02 05 06]
        // This is the SAME as Frame 2! So should be GameCache
        manager.process_client_input(0, ClientInput::GameCache(0)); // [01 02]
        let outputs = manager.process_client_input(1, ClientInput::GameCache(1)); // [05 06]

        assert_eq!(outputs.len(), 2);
        // Combined [01 02 05 06] already seen in Frame 2 → GameCache
        assert!(
            matches!(outputs[0].response, ServerResponse::GameCache(1)),
            "Frame 4: P0 should get GameCache (same as Frame 2)"
        );
        assert!(
            matches!(outputs[1].response, ServerResponse::GameCache(1)),
            "Frame 4: P1 should get GameCache (same as Frame 2)"
        );

        // Frame 5: GC GC - P0[07 08] + P1[05 06] → Combined [07 08 05 06]
        // This is NEW! Never seen this combination before!
        manager.process_client_input(0, ClientInput::GameCache(1)); // [07 08]
        let outputs = manager.process_client_input(1, ClientInput::GameCache(1)); // [05 06]

        assert_eq!(outputs.len(), 2);
        // Combined [07 08 05 06] is NEW even though both used cache → GameData!
        match &outputs[0].response {
            ServerResponse::GameData(data) => {
                assert_eq!(
                    data,
                    &vec![0x07, 0x08, 0x05, 0x06],
                    "Frame 5: NEW combined from GC GC!"
                );
            }
            ServerResponse::GameCache(_) => panic!("Frame 5: Should be GameData (new combination)"),
        }
        match &outputs[1].response {
            ServerResponse::GameData(data) => {
                assert_eq!(data, &vec![0x07, 0x08, 0x05, 0x06]);
            }
            ServerResponse::GameCache(_) => panic!("Frame 5: Should be GameData (new combination)"),
        }

        // Frame 6: GC GC - Repeat Frame 5 exactly → Should be GameCache now
        manager.process_client_input(0, ClientInput::GameCache(1)); // [07 08]
        let outputs = manager.process_client_input(1, ClientInput::GameCache(1)); // [05 06]

        assert_eq!(outputs.len(), 2);
        // Now [07 08 05 06] is in cache → GameCache
        assert!(
            matches!(outputs[0].response, ServerResponse::GameCache(3)),
            "Frame 6: P0 should get GameCache (same as Frame 5)"
        );
        assert!(
            matches!(outputs[1].response, ServerResponse::GameCache(3)),
            "Frame 6: P1 should get GameCache (same as Frame 5)"
        );
    }

    #[test]
    #[should_panic(expected = "Player 0 delay must be positive")]
    fn test_invalid_zero_delay() {
        GameSyncManager::new(vec![0, 1]);
    }

    #[test]
    #[should_panic(expected = "Player 0 input data must be 2 bytes (delay 1 * 2), got 3")]
    fn test_invalid_input_size() {
        let mut manager = GameSyncManager::new(vec![1, 1]);
        manager.process_client_input(0, ClientInput::GameData(vec![0x01, 0x02, 0x03]));
    }

    #[test]
    fn test_delay_2_with_4_bytes_succeeds() {
        // Test that delay 2 player can send 4 bytes (2 frames worth)
        let mut manager = GameSyncManager::new(vec![2, 2]);

        // P0 sends 4 bytes (delay 2 * 2)
        manager.process_client_input(0, ClientInput::GameData(vec![0xAA, 0xBB, 0xCC, 0xDD]));
        // P1 sends 4 bytes
        let outputs =
            manager.process_client_input(1, ClientInput::GameData(vec![0x11, 0x22, 0x33, 0x44]));

        // Both players should receive output
        assert_eq!(outputs.len(), 2);

        // Combined output should be: [AA BB CC DD 11 22 33 44]
        // P0's 4 bytes + P1's 4 bytes
        match &outputs[0].response {
            ServerResponse::GameData(data) => {
                assert_eq!(data, &vec![0xAA, 0xBB, 0xCC, 0xDD, 0x11, 0x22, 0x33, 0x44]);
            }
            _ => panic!("Should get GameData"),
        }
    }
}
