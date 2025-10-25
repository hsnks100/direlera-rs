// Simple Game Sync - Cleaner implementation with per-player send buffers
// Each player has their own independent send buffer

use std::collections::VecDeque;

/// Client input message type
#[derive(Debug, Clone, PartialEq)]
pub enum ClientInput {
    /// Game Data: contains the actual input bytes
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

/// Output action for a specific player
#[derive(Debug, Clone, PartialEq)]
pub struct PlayerOutput {
    pub player_id: usize,
    pub response: ServerResponse,
}

/// FIFO cache with 256 slots (rolls over to 0 when full)
#[derive(Debug, Clone)]
struct InputCache {
    slots: VecDeque<Vec<u8>>,
}

impl InputCache {
    fn new() -> Self {
        Self {
            slots: VecDeque::with_capacity(256),
        }
    }

    /// Find data in cache, returning the position if found
    fn find(&self, data: &[u8]) -> Option<u8> {
        self.slots
            .iter()
            .enumerate()
            .rev()
            .find(|(_, cached)| cached.as_slice() == data)
            .map(|(idx, _)| idx as u8)
    }

    /// Add data to cache (rolls over at 256)
    fn push(&mut self, data: Vec<u8>) {
        if self.slots.len() >= 256 {
            self.slots.pop_front();
        }
        self.slots.push_back(data);
    }

    /// Get data at position
    fn get(&self, pos: u8) -> Option<&[u8]> {
        self.slots.get(pos as usize).map(|v| v.as_slice())
    }
}

/// Per-player input state
#[derive(Debug, Clone)]
struct PlayerInput {
    /// Input frames (2-byte chunks)
    frames: Vec<Vec<u8>>,
    /// Client's input cache
    client_cache: InputCache,
    /// Expected input size (delay * 2)
    input_size: usize,
    /// Delay value
    delay: usize,
    /// Number of frames already distributed to send buffers
    distributed_count: usize,
}

impl PlayerInput {
    fn new(delay: usize) -> Self {
        Self {
            frames: Vec::new(),
            client_cache: InputCache::new(),
            input_size: delay * 2,
            delay,
            distributed_count: 0,
        }
    }

    /// Add input (splits into 2-byte chunks)
    fn add_input(&mut self, data: Vec<u8>) {
        for chunk in data.chunks(2) {
            if chunk.len() == 2 {
                self.frames.push(chunk.to_vec());
            }
        }
    }
}

/// Per-player output state
#[derive(Debug, Clone)]
struct PlayerOutputState {
    /// Send buffer: holds frames to send to this player
    /// Each sub-vec is for one source player's frames
    send_buffers: Vec<VecDeque<Vec<u8>>>,
    /// Output cache (combined data this player has received)
    output_cache: InputCache,
    /// Delay value
    delay: usize,
}

impl PlayerOutputState {
    fn new(player_count: usize, delay: usize) -> Self {
        Self {
            send_buffers: (0..player_count).map(|_| VecDeque::new()).collect(),
            output_cache: InputCache::new(),
            delay,
        }
    }

    /// Check if ready to send
    fn can_send(&self) -> bool {
        self.send_buffers.iter().all(|buf| buf.len() >= self.delay)
    }

    /// Extract and combine frames
    fn extract_combined(&mut self) -> Vec<u8> {
        let mut combined = Vec::new();
        for _ in 0..self.delay {
            for buf in &mut self.send_buffers {
                if let Some(frame) = buf.pop_front() {
                    combined.extend_from_slice(&frame);
                }
            }
        }
        combined
    }
}

/// Simple Game Sync Manager
#[derive(Debug, Clone)]
pub struct SimpleGameSync {
    player_count: usize,
    inputs: Vec<PlayerInput>,
    outputs: Vec<PlayerOutputState>,
    #[allow(dead_code)]
    use_preemptive_padding: bool,
}

impl SimpleGameSync {
    /// Create new sync manager with preemptive padding enabled (default)
    pub fn new(delays: Vec<usize>) -> Self {
        Self::new_with_options(delays, true)
    }

    /// Create new sync manager without preemptive padding
    pub fn new_without_padding(delays: Vec<usize>) -> Self {
        Self::new_with_options(delays, false)
    }

    /// Create new sync manager with custom options
    pub fn new_with_options(delays: Vec<usize>, use_preemptive_padding: bool) -> Self {
        let player_count = delays.len();
        assert!(player_count >= 1, "At least 1 player required");

        for (i, &delay) in delays.iter().enumerate() {
            assert!(delay > 0, "Player {} delay must be positive", i);
        }

        let min_delay = *delays.iter().min().unwrap();

        let inputs: Vec<_> = delays.iter().map(|&d| PlayerInput::new(d)).collect();
        let outputs: Vec<_> = delays
            .iter()
            .map(|&d| PlayerOutputState::new(player_count, d))
            .collect();

        let mut manager = Self {
            player_count,
            inputs,
            outputs,
            use_preemptive_padding,
        };

        if use_preemptive_padding {
            // Apply preemptive padding
            for (i, &delay) in delays.iter().enumerate() {
                let padding = delay - min_delay;
                for _ in 0..padding {
                    manager.inputs[i].add_input(vec![0x00, 0x00]);
                }
            }

            // Distribute padding to all send buffers
            for i in 0..player_count {
                let frame_count = manager.inputs[i].frames.len();
                for frame_idx in 0..frame_count {
                    let frame = manager.inputs[i].frames[frame_idx].clone();
                    for output in &mut manager.outputs {
                        output.send_buffers[i].push_back(frame.clone());
                    }
                }
                manager.inputs[i].distributed_count = frame_count;
            }
        }

        manager
    }

    /// Process client input and return outputs
    pub fn process_client_input(
        &mut self,
        player_id: usize,
        input: ClientInput,
    ) -> Vec<PlayerOutput> {
        assert!(player_id < self.player_count, "Invalid player_id");

        let expected_size = self.inputs[player_id].input_size;

        // Resolve input data
        let input_data = match input {
            ClientInput::GameData(data) => {
                assert_eq!(
                    data.len(),
                    expected_size,
                    "Player {} input data must be {} bytes (delay {} * 2), got {}",
                    player_id,
                    expected_size,
                    self.inputs[player_id].delay,
                    data.len()
                );
                self.inputs[player_id].client_cache.push(data.clone());
                data
            }
            ClientInput::GameCache(pos) => self.inputs[player_id]
                .client_cache
                .get(pos)
                .expect("Cache position not found")
                .to_vec(),
        };

        // Add to input buffer
        self.inputs[player_id].add_input(input_data);

        // Distribute new frames to all players' send buffers
        let start_idx = self.inputs[player_id].distributed_count;
        for frame_idx in start_idx..self.inputs[player_id].frames.len() {
            let frame = self.inputs[player_id].frames[frame_idx].clone();
            for output in &mut self.outputs {
                output.send_buffers[player_id].push_back(frame.clone());
            }
        }
        self.inputs[player_id].distributed_count = self.inputs[player_id].frames.len();

        // Try to generate outputs
        self.try_generate_outputs()
    }

    /// Try to generate outputs for all ready players
    fn try_generate_outputs(&mut self) -> Vec<PlayerOutput> {
        let mut results = Vec::new();

        loop {
            // Find players ready to send
            let ready: Vec<_> = (0..self.player_count)
                .filter(|&i| self.outputs[i].can_send())
                .collect();

            if ready.is_empty() {
                break;
            }

            // Find minimum delay among ready players
            let min_delay = ready.iter().map(|&i| self.outputs[i].delay).min().unwrap();

            // Process all players with minimum delay
            for &player_id in &ready {
                if self.outputs[player_id].delay == min_delay {
                    let combined = self.outputs[player_id].extract_combined();

                    let response =
                        if let Some(pos) = self.outputs[player_id].output_cache.find(&combined) {
                            ServerResponse::GameCache(pos)
                        } else {
                            self.outputs[player_id].output_cache.push(combined.clone());
                            ServerResponse::GameData(combined)
                        };

                    results.push(PlayerOutput {
                        player_id,
                        response,
                    });
                }
            }
        }

        results
    }

    /// Get player delay
    pub fn get_player_delay(&self, player_id: usize) -> usize {
        assert!(player_id < self.player_count, "Invalid player_id");
        self.inputs[player_id].delay
    }

    /// Get player count
    pub fn player_count(&self) -> usize {
        self.player_count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_equal_delays() {
        let mut manager = SimpleGameSync::new(vec![1, 1]);

        // Frame 1: P0 sends input
        let outputs = manager.process_client_input(0, ClientInput::GameData(vec![0x01, 0x02]));
        assert_eq!(outputs.len(), 0);

        // Frame 1: P1 sends input
        let outputs = manager.process_client_input(1, ClientInput::GameData(vec![0x03, 0x04]));
        assert_eq!(outputs.len(), 2);

        assert_eq!(outputs[0].player_id, 0);
        assert_eq!(outputs[1].player_id, 1);

        match &outputs[0].response {
            ServerResponse::GameData(data) => {
                assert_eq!(data, &vec![0x01, 0x02, 0x03, 0x04]);
            }
            _ => panic!("P0's first output should be GameData"),
        }

        match &outputs[1].response {
            ServerResponse::GameData(data) => {
                assert_eq!(data, &vec![0x01, 0x02, 0x03, 0x04]);
            }
            _ => panic!("P1's first output should be GameData"),
        }

        // Frame 2: Both send same inputs via cache
        manager.process_client_input(0, ClientInput::GameCache(0));
        let outputs = manager.process_client_input(1, ClientInput::GameCache(0));

        assert_eq!(outputs.len(), 2);
        assert!(matches!(outputs[0].response, ServerResponse::GameCache(_)));
        assert!(matches!(outputs[1].response, ServerResponse::GameCache(_)));
    }

    #[test]
    fn test_different_delays() {
        let mut manager = SimpleGameSync::new(vec![1, 2]);

        // P0 sends first input
        let outputs = manager.process_client_input(0, ClientInput::GameData(vec![0x01, 0x00]));
        assert_eq!(outputs.len(), 1);
        assert_eq!(outputs[0].player_id, 0);

        // P0 sends second and third
        manager.process_client_input(0, ClientInput::GameData(vec![0x02, 0x00]));
        let outputs = manager.process_client_input(0, ClientInput::GameData(vec![0x03, 0x00]));
        assert_eq!(outputs.len(), 0);

        // P1 sends 4 bytes
        let outputs =
            manager.process_client_input(1, ClientInput::GameData(vec![0xAA, 0xBB, 0xCC, 0xDD]));
        assert!(outputs.len() >= 2);

        let p0_outputs: Vec<_> = outputs.iter().filter(|o| o.player_id == 0).collect();
        let p1_outputs: Vec<_> = outputs.iter().filter(|o| o.player_id == 1).collect();

        assert_eq!(p0_outputs.len(), 2);
        assert_eq!(p1_outputs.len(), 1);
    }

    #[test]
    fn test_cache_mechanism() {
        let mut manager = SimpleGameSync::new(vec![1, 1]);

        manager.process_client_input(0, ClientInput::GameData(vec![0x00, 0x00]));
        let outputs = manager.process_client_input(1, ClientInput::GameData(vec![0x00, 0x00]));

        assert_eq!(outputs.len(), 2);
        assert!(matches!(outputs[0].response, ServerResponse::GameData(_)));

        manager.process_client_input(0, ClientInput::GameCache(0));
        let outputs = manager.process_client_input(1, ClientInput::GameCache(0));

        let has_cache = outputs
            .iter()
            .any(|o| matches!(o.response, ServerResponse::GameCache(_)));
        assert!(has_cache);
    }

    #[test]
    fn test_three_players() {
        let mut manager = SimpleGameSync::new(vec![1, 1, 2]);

        manager.process_client_input(0, ClientInput::GameData(vec![0x01, 0x00]));
        let outputs = manager.process_client_input(1, ClientInput::GameData(vec![0x02, 0x00]));

        assert_eq!(outputs.len(), 2);
        assert_eq!(outputs[0].player_id, 0);
        assert_eq!(outputs[1].player_id, 1);
    }

    #[test]
    fn test_preemptive_padding() {
        let mut manager = SimpleGameSync::new(vec![1, 3]);

        let outputs = manager.process_client_input(0, ClientInput::GameData(vec![0x01, 0x00]));
        assert_eq!(outputs.len(), 1);

        let outputs = manager.process_client_input(0, ClientInput::GameData(vec![0x02, 0x00]));
        assert_eq!(outputs.len(), 1);
    }

    #[test]
    fn test_gd_gc_pattern_delay_1() {
        let mut manager = SimpleGameSync::new(vec![1, 1]);

        // Frame 1
        manager.process_client_input(0, ClientInput::GameData(vec![0xAA, 0xBB]));
        let outputs = manager.process_client_input(1, ClientInput::GameData(vec![0xCC, 0xDD]));
        assert_eq!(outputs.len(), 2);

        // Frame 2
        manager.process_client_input(0, ClientInput::GameCache(0));
        let outputs = manager.process_client_input(1, ClientInput::GameCache(0));
        assert!(matches!(outputs[0].response, ServerResponse::GameCache(_)));

        // Frame 3
        manager.process_client_input(0, ClientInput::GameCache(0));
        let outputs = manager.process_client_input(1, ClientInput::GameCache(0));
        assert!(matches!(outputs[0].response, ServerResponse::GameCache(_)));

        // Frame 4
        manager.process_client_input(0, ClientInput::GameData(vec![0x11, 0x22]));
        let outputs = manager.process_client_input(1, ClientInput::GameData(vec![0x33, 0x44]));
        assert!(matches!(outputs[0].response, ServerResponse::GameData(_)));
    }

    #[test]
    fn test_gd_gc_pattern_delay_2() {
        let mut manager = SimpleGameSync::new(vec![2, 2]);

        manager.process_client_input(0, ClientInput::GameData(vec![0xAA, 0xBB, 0xAA, 0xBB]));
        let outputs =
            manager.process_client_input(1, ClientInput::GameData(vec![0xCC, 0xDD, 0xCC, 0xDD]));
        assert_eq!(outputs.len(), 2);

        manager.process_client_input(0, ClientInput::GameCache(0));
        let outputs = manager.process_client_input(1, ClientInput::GameCache(0));
        assert!(matches!(outputs[0].response, ServerResponse::GameCache(_)));
    }

    #[test]
    fn test_gd_gc_creates_new_combined() {
        let mut manager = SimpleGameSync::new(vec![1, 1]);

        manager.process_client_input(0, ClientInput::GameData(vec![0x01, 0x02]));
        manager.process_client_input(1, ClientInput::GameData(vec![0x03, 0x04]));

        manager.process_client_input(0, ClientInput::GameData(vec![0x05, 0x06]));
        let outputs = manager.process_client_input(1, ClientInput::GameCache(0));

        match &outputs[0].response {
            ServerResponse::GameData(data) => {
                assert_eq!(data, &vec![0x05, 0x06, 0x03, 0x04]);
            }
            _ => panic!("Should be GameData"),
        }
    }

    #[test]
    fn test_gc_gc_creates_new_combined() {
        let mut manager = SimpleGameSync::new(vec![1, 1]);

        manager.process_client_input(0, ClientInput::GameData(vec![0x01, 0x02]));
        manager.process_client_input(1, ClientInput::GameData(vec![0x03, 0x04]));

        manager.process_client_input(0, ClientInput::GameCache(0));
        manager.process_client_input(1, ClientInput::GameData(vec![0x05, 0x06]));

        manager.process_client_input(0, ClientInput::GameData(vec![0x07, 0x08]));
        manager.process_client_input(1, ClientInput::GameCache(0));

        manager.process_client_input(0, ClientInput::GameCache(0));
        let outputs = manager.process_client_input(1, ClientInput::GameCache(1));
        assert!(matches!(outputs[0].response, ServerResponse::GameCache(_)));

        manager.process_client_input(0, ClientInput::GameCache(1));
        let outputs = manager.process_client_input(1, ClientInput::GameCache(1));
        match &outputs[0].response {
            ServerResponse::GameData(_) => {}
            _ => panic!("Should be GameData (new combination)"),
        }
    }

    #[test]
    fn test_delay_2_with_4_bytes_succeeds() {
        let mut manager = SimpleGameSync::new(vec![2, 2]);

        manager.process_client_input(0, ClientInput::GameData(vec![0xAA, 0xBB, 0xCC, 0xDD]));
        let outputs =
            manager.process_client_input(1, ClientInput::GameData(vec![0x11, 0x22, 0x33, 0x44]));

        assert_eq!(outputs.len(), 2);
    }

    #[test]
    #[should_panic(expected = "Player 0 delay must be positive")]
    fn test_invalid_zero_delay() {
        SimpleGameSync::new(vec![0, 1]);
    }

    #[test]
    #[should_panic(expected = "Player 0 input data must be 2 bytes (delay 1 * 2), got 3")]
    fn test_invalid_input_size() {
        let mut manager = SimpleGameSync::new(vec![1, 1]);
        manager.process_client_input(0, ClientInput::GameData(vec![0x01, 0x02, 0x03]));
    }

    #[test]
    fn test_without_preemptive_padding() {
        // Create manager without padding
        let mut manager = SimpleGameSync::new_without_padding(vec![1, 2]);

        // P0 sends first input - no output (P1 has no data yet, not even padding)
        let outputs = manager.process_client_input(0, ClientInput::GameData(vec![0x01, 0x00]));
        assert_eq!(outputs.len(), 0);

        // P0 sends second input - still no output
        let outputs = manager.process_client_input(0, ClientInput::GameData(vec![0x02, 0x00]));
        assert_eq!(outputs.len(), 0);

        // P1 finally sends input (4 bytes for delay 2)
        let outputs =
            manager.process_client_input(1, ClientInput::GameData(vec![0xAA, 0xBB, 0xCC, 0xDD]));

        // Now both players can receive
        let p0_outputs: Vec<_> = outputs.iter().filter(|o| o.player_id == 0).collect();
        let p1_outputs: Vec<_> = outputs.iter().filter(|o| o.player_id == 1).collect();

        // P0 should get 2 frames (frames 1-2)
        assert_eq!(p0_outputs.len(), 2);
        // P1 should get 1 batch (frames 1-2 combined)
        assert_eq!(p1_outputs.len(), 1);

        // Verify P0's frames
        match &p0_outputs[0].response {
            ServerResponse::GameData(data) => {
                // [P0_F1][P1_F1] = [01 00][AA BB]
                assert_eq!(data, &vec![0x01, 0x00, 0xAA, 0xBB]);
            }
            _ => panic!("Should be GameData"),
        }
        match &p0_outputs[1].response {
            ServerResponse::GameData(data) => {
                // [P0_F2][P1_F2] = [02 00][CC DD]
                assert_eq!(data, &vec![0x02, 0x00, 0xCC, 0xDD]);
            }
            _ => panic!("Should be GameData"),
        }

        // Verify P1's combined frames
        match &p1_outputs[0].response {
            ServerResponse::GameData(data) => {
                // [P0_F1][P1_F1][P0_F2][P1_F2] = [01 00][AA BB][02 00][CC DD]
                assert_eq!(data, &vec![0x01, 0x00, 0xAA, 0xBB, 0x02, 0x00, 0xCC, 0xDD]);
            }
            _ => panic!("Should be GameData"),
        }
    }

    #[test]
    fn test_with_vs_without_padding_difference() {
        // WITH padding: P0 gets output immediately
        let mut with_padding = SimpleGameSync::new(vec![1, 2]);
        let outputs = with_padding.process_client_input(0, ClientInput::GameData(vec![0x01, 0x00]));
        assert_eq!(
            outputs.len(),
            1,
            "With padding: P0 should get immediate output"
        );

        // WITHOUT padding: P0 waits for P1
        let mut without_padding = SimpleGameSync::new_without_padding(vec![1, 2]);
        let outputs =
            without_padding.process_client_input(0, ClientInput::GameData(vec![0x01, 0x00]));
        assert_eq!(
            outputs.len(),
            0,
            "Without padding: P0 must wait for P1's first input"
        );
    }
}
