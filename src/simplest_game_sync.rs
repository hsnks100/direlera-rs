// Simple Game Sync - Cleaner implementation with per-player send buffers
// Each player has their own independent send buffer

#![allow(dead_code)]

// logic description

// 1.  **동시 입력 대기:** 시스템은 **모든 플레이어(P1, P2, P3)**의 인풋 버퍼에 값이 채워지기를 기다립니다. 하나라도 누락되면 다음 단계로 넘어가지 않습니다.
// 2.  **묶음 생성 및 이동:** 모든 인풋이 확인되면, 이들을 하나의 **인풋 묶음(Bundle)**으로 만듭니다.
// 3.  **전송 버퍼 누적:** 생성된 인풋 묶음은 **모든 플레이어의 전송 버퍼**에 **동일하게** 추가되어 누적됩니다. 이후 인풋 버퍼는 비워집니다.
// 4.  **전송 가능 확인:** 각 플레이어 $P_i$ 마다, 자신의 전송 버퍼에 누적된 묶음의 개수가 미리 정해진 **최소 전송 단위**($N_i$)에 도달했는지 확인합니다.
// 5.  **데이터 전송:** $N_i$를 충족한 플레이어에게는 버퍼에 누적된 **전체 인풋 묶음**을 전송(리턴)합니다.
// 6.  **버퍼 초기화:** 전송이 완료된 해당 플레이어의 전송 버퍼는 **즉시 비워집니다.**

use std::collections::VecDeque;

use crate::simple_game_sync::{ClientInput, InputCache, ServerResponse};

type InputData = Vec<u8>;
const MINIMUM_INPUT_SIZE: usize = 2;

/// Error types for game sync operations
#[derive(Debug, Clone, PartialEq)]
pub enum GameSyncError {
    /// Invalid player ID (out of range)
    InvalidPlayerId {
        player_id: usize,
        player_count: usize,
    },
    /// Cache position not found
    CachePositionNotFound { player_id: usize, position: u8 },
    /// Internal buffer inconsistency (should not happen in normal operation)
    BufferInconsistency { message: String },
}

impl std::fmt::Display for GameSyncError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GameSyncError::InvalidPlayerId {
                player_id,
                player_count,
            } => write!(
                f,
                "Invalid player_id: {} (valid range: 0..{})",
                player_id, player_count
            ),
            GameSyncError::CachePositionNotFound {
                player_id,
                position,
            } => {
                write!(
                    f,
                    "Cache position {} not found for player {}",
                    position, player_id
                )
            }
            GameSyncError::BufferInconsistency { message } => {
                write!(f, "Buffer inconsistency: {}", message)
            }
        }
    }
}

impl std::error::Error for GameSyncError {}

/// Output action for a specific player with cache support
#[derive(Debug, Clone, PartialEq)]
pub struct CachedPlayerOutput {
    pub player_id: usize,
    pub response: ServerResponse,
}
#[derive(Debug, Clone)]
pub struct SimplestGameSync {
    input_buffers: Vec<VecDeque<InputData>>,
    output_buffers: Vec<VecDeque<u8>>,
    player_delays: Vec<usize>,
    dropped_players: Vec<bool>,
}

#[derive(Debug, PartialEq, Clone)]
pub struct PlayerOutput {
    pub player_id: usize,
    pub output: Vec<u8>,
}
impl SimplestGameSync {
    pub fn new(player_delays: Vec<usize>) -> Self {
        let player_count = player_delays.len();
        Self {
            input_buffers: vec![VecDeque::new(); player_count],
            output_buffers: vec![VecDeque::new(); player_count],
            player_delays,
            dropped_players: vec![false; player_count],
        }
    }

    pub fn process_client_input(
        &mut self,
        player_id: usize,
        input: InputData,
    ) -> Result<Vec<PlayerOutput>, GameSyncError> {
        // Validate player_id
        if player_id >= self.input_buffers.len() {
            return Err(GameSyncError::InvalidPlayerId {
                player_id,
                player_count: self.input_buffers.len(),
            });
        }

        // split input into 2-byte chunks
        for chunk in input.chunks(MINIMUM_INPUT_SIZE) {
            if chunk.len() == MINIMUM_INPUT_SIZE {
                self.input_buffers[player_id].push_back(chunk.to_vec());
            }
        }
        // process bundles while all input buffers have at least one item (or player is dropped)
        while self
            .input_buffers
            .iter()
            .enumerate()
            .all(|(i, buffer)| !buffer.is_empty() || self.dropped_players[i])
        {
            // Before creating bundle, fill empty buffers for dropped players with [0x00, 0x00]
            for (player_id, buffer) in self.input_buffers.iter_mut().enumerate() {
                if self.dropped_players[player_id] && buffer.is_empty() {
                    buffer.push_back(vec![0x00, 0x00]);
                }
            }
            // create a bundle by taking one item from each input buffer
            let mut bundle = Vec::with_capacity(self.input_buffers.len());
            for buffer in &mut self.input_buffers {
                // Safe to unwrap here because we checked all buffers are non-empty or dropped
                if let Some(item) = buffer.pop_front() {
                    bundle.push(item);
                } else {
                    return Err(GameSyncError::BufferInconsistency {
                        message: "Buffer became empty during bundle creation".to_string(),
                    });
                }
            }
            // serialize bundle into a single Vec<u8> for storage
            let serialized_bundle: InputData = bundle.into_iter().flatten().collect();
            // add the bundle to all output buffers

            for output_buffer in &mut self.output_buffers {
                output_buffer.extend(serialized_bundle.clone());
            }
        }
        let mut results = Vec::new();
        for player_id in 0..self.player_delays.len() {
            if self.output_buffers[player_id].len()
                >= self.player_delays[player_id] * self.player_delays.len() * MINIMUM_INPUT_SIZE
            {
                let output = self.output_buffers[player_id]
                    .drain(
                        ..self.player_delays[player_id]
                            * self.player_delays.len()
                            * MINIMUM_INPUT_SIZE,
                    )
                    .collect::<Vec<u8>>();
                results.push(PlayerOutput { player_id, output });
            }
        }
        Ok(results)
    }

    /// Get player delays (for wrapper layer access)
    pub(crate) fn player_delays(&self) -> &[usize] {
        &self.player_delays
    }

    /// Mark a player as dropped
    pub fn mark_player_dropped(&mut self, player_id: usize) -> Result<(), GameSyncError> {
        if player_id >= self.dropped_players.len() {
            return Err(GameSyncError::InvalidPlayerId {
                player_id,
                player_count: self.dropped_players.len(),
            });
        }
        self.dropped_players[player_id] = true;
        Ok(())
    }

    /// Check if a player is dropped
    pub fn is_player_dropped(&self, player_id: usize) -> bool {
        player_id < self.dropped_players.len() && self.dropped_players[player_id]
    }
}

/// Wrapper layer that adds GameCache support to SimplestGameSync
#[derive(Debug, Clone)]
pub struct CachedGameSync {
    /// Core sync engine without cache
    sync: SimplestGameSync,
    /// Per-player input caches (client-side cache)
    input_caches: Vec<InputCache>,
    /// Per-player output caches (server-side cache)
    output_caches: Vec<InputCache>,
}

impl CachedGameSync {
    /// Create a new cached game sync manager
    pub fn new(player_delays: Vec<usize>) -> Self {
        let player_count = player_delays.len();
        Self {
            sync: SimplestGameSync::new(player_delays.clone()),
            input_caches: (0..player_count).map(|_| InputCache::new()).collect(),
            output_caches: (0..player_count).map(|_| InputCache::new()).collect(),
        }
    }

    /// Process client input with cache support
    pub fn process_client_input(
        &mut self,
        player_id: usize,
        input: ClientInput,
    ) -> Result<Vec<CachedPlayerOutput>, GameSyncError> {
        // Validate player_id
        let player_count = self.sync.player_delays().len();
        if player_id >= player_count {
            return Err(GameSyncError::InvalidPlayerId {
                player_id,
                player_count,
            });
        }

        // Resolve input data from cache if needed
        let input_data = match input {
            ClientInput::GameData(data) => {
                // Store in client's input cache
                self.input_caches[player_id].push(data.clone());
                data
            }
            ClientInput::GameCache(pos) => self.input_caches[player_id]
                .get(pos)
                .ok_or_else(|| GameSyncError::CachePositionNotFound {
                    player_id,
                    position: pos,
                })?
                .to_vec(),
        };

        // Process with core sync engine
        let raw_outputs = self.sync.process_client_input(player_id, input_data)?;

        // Convert outputs to cached responses
        let mut results = Vec::new();
        for raw_output in raw_outputs {
            let cached_output = if let Some(cache_pos) =
                self.output_caches[raw_output.player_id].find(&raw_output.output)
            {
                // Found in cache - use cache reference
                ServerResponse::GameCache(cache_pos)
            } else {
                // Not in cache - store and return full data
                self.output_caches[raw_output.player_id].push(raw_output.output.clone());
                ServerResponse::GameData(raw_output.output)
            };

            results.push(CachedPlayerOutput {
                player_id: raw_output.player_id,
                response: cached_output,
            });
        }

        Ok(results)
    }

    /// Get player count
    pub fn player_count(&self) -> usize {
        self.sync.player_delays().len()
    }

    /// Get player delay
    pub fn get_player_delay(&self, player_id: usize) -> usize {
        self.sync.player_delays()[player_id]
    }

    /// Mark a player as dropped
    pub fn mark_player_dropped(&mut self, player_id: usize) -> Result<(), GameSyncError> {
        self.sync.mark_player_dropped(player_id)
    }

    /// Check if a player is dropped
    pub fn is_player_dropped(&self, player_id: usize) -> bool {
        self.sync.is_player_dropped(player_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_equal_delays() {
        let mut sync = SimplestGameSync::new(vec![1, 1]);

        // Frame 1: P0 sends input (2 bytes = 1 chunk)
        let outputs = sync.process_client_input(0, vec![0x01, 0x02]).unwrap();
        assert_eq!(outputs.len(), 0, "No output until all players have input");

        // Frame 1: P1 sends input (2 bytes = 1 chunk)
        let outputs = sync.process_client_input(1, vec![0x03, 0x04]).unwrap();
        assert_eq!(outputs.len(), 2, "Both players should get output");

        // Verify outputs contain combined input from both players
        assert_eq!(outputs[0].player_id, 0);
        assert_eq!(outputs[0].output, vec![0x01, 0x02, 0x03, 0x04]);

        assert_eq!(outputs[1].player_id, 1);
        assert_eq!(outputs[1].output, vec![0x01, 0x02, 0x03, 0x04]);

        // Frame 2: Both players send new inputs
        sync.process_client_input(0, vec![0x05, 0x06]).unwrap();
        let outputs = sync.process_client_input(1, vec![0x07, 0x08]).unwrap();
        assert_eq!(outputs.len(), 2);
        assert_eq!(outputs[0].output, vec![0x05, 0x06, 0x07, 0x08]);
    }

    #[test]
    fn test_different_delays() {
        let mut sync = SimplestGameSync::new(vec![1, 2]);

        // P0 sends first input (delay 1, needs 1 bundle = 2 players * 1 * 2 bytes = 4 bytes)
        let outputs = sync.process_client_input(0, vec![0x01, 0x02]).unwrap();
        assert_eq!(outputs.len(), 0, "P0 waits for P1");

        // P1 sends first input (4 bytes = 2 chunks)
        let outputs = sync
            .process_client_input(1, vec![0x03, 0x04, 0x05, 0x06])
            .unwrap();
        // First bundle created: [0x01, 0x02] + [0x03, 0x04] = [0x01, 0x02, 0x03, 0x04]
        // P0 buffer now has 4 bytes (1 bundle), which is >= 1 * 2 * 2 = 4, so P0 outputs
        // P1 buffer now has 4 bytes (1 bundle), which is < 2 * 2 * 2 = 8, so no output
        assert_eq!(outputs.len(), 1);
        assert_eq!(outputs[0].player_id, 0);
        assert_eq!(outputs[0].output, vec![0x01, 0x02, 0x03, 0x04]);

        // P0 sends second input
        let outputs = sync.process_client_input(0, vec![0x07, 0x08]).unwrap();
        // Second bundle created: [0x07, 0x08] + [0x05, 0x06] = [0x07, 0x08, 0x05, 0x06]
        // P0 buffer now has 4 bytes (1 bundle), which is >= 4, so P0 outputs
        // P1 buffer now has 8 bytes (2 bundles), which is >= 8, so P1 outputs
        assert_eq!(outputs.len(), 2);
        let p0_output = outputs.iter().find(|o| o.player_id == 0).unwrap();
        let p1_output = outputs.iter().find(|o| o.player_id == 1).unwrap();
        assert_eq!(p0_output.output, vec![0x07, 0x08, 0x05, 0x06]);
        // P1 should get both bundles combined: [0x01, 0x02, 0x03, 0x04, 0x07, 0x08, 0x05, 0x06]
        assert_eq!(p1_output.output.len(), 8);
    }

    #[test]
    fn test_three_players() {
        let mut sync = SimplestGameSync::new(vec![1, 1, 1]);

        // All players send first input
        sync.process_client_input(0, vec![0x01, 0x02]).unwrap();
        sync.process_client_input(1, vec![0x03, 0x04]).unwrap();
        let outputs = sync.process_client_input(2, vec![0x05, 0x06]).unwrap();

        // All players should get output with all 3 inputs combined
        assert_eq!(outputs.len(), 3);
        for output in &outputs {
            assert_eq!(output.output, vec![0x01, 0x02, 0x03, 0x04, 0x05, 0x06]);
        }
    }

    #[test]
    fn test_multiple_chunks_single_input() {
        let mut sync = SimplestGameSync::new(vec![1, 1]);

        // P0 sends 4 bytes (2 chunks)
        sync.process_client_input(0, vec![0x01, 0x02, 0x03, 0x04])
            .unwrap();
        // P1 sends 2 bytes (1 chunk)
        let outputs = sync.process_client_input(1, vec![0x05, 0x06]).unwrap();

        // First bundle: [0x01, 0x02] + [0x05, 0x06]
        // P0 still has [0x03, 0x04] in buffer, so no second bundle yet
        assert_eq!(outputs.len(), 2);
        assert_eq!(outputs[0].output, vec![0x01, 0x02, 0x05, 0x06]);
        assert_eq!(outputs[1].output, vec![0x01, 0x02, 0x05, 0x06]);

        // P1 sends more to create second bundle
        let outputs = sync.process_client_input(1, vec![0x07, 0x08]).unwrap();
        // Second bundle: [0x03, 0x04] + [0x07, 0x08]
        assert_eq!(outputs.len(), 2);
        assert_eq!(outputs[0].output, vec![0x03, 0x04, 0x07, 0x08]);
    }

    #[test]
    fn test_sequential_inputs() {
        let mut sync = SimplestGameSync::new(vec![1, 1]);

        // P0 sends multiple inputs before P1 responds
        sync.process_client_input(0, vec![0x01, 0x02]).unwrap();
        sync.process_client_input(0, vec![0x03, 0x04]).unwrap();
        sync.process_client_input(0, vec![0x05, 0x06]).unwrap();

        // P1 sends first input - should create first bundle
        let outputs = sync.process_client_input(1, vec![0x07, 0x08]).unwrap();
        assert_eq!(outputs.len(), 2);
        assert_eq!(outputs[0].output, vec![0x01, 0x02, 0x07, 0x08]);

        // P1 sends second input - should create second bundle
        let outputs = sync.process_client_input(1, vec![0x09, 0x0A]).unwrap();
        assert_eq!(outputs.len(), 2);
        assert_eq!(outputs[0].output, vec![0x03, 0x04, 0x09, 0x0A]);

        // P1 sends third input - should create third bundle
        let outputs = sync.process_client_input(1, vec![0x0B, 0x0C]).unwrap();
        assert_eq!(outputs.len(), 2);
        assert_eq!(outputs[0].output, vec![0x05, 0x06, 0x0B, 0x0C]);
    }

    #[test]
    fn test_uneven_distribution() {
        let mut sync = SimplestGameSync::new(vec![1, 2, 1]);

        // P0 sends input
        sync.process_client_input(0, vec![0x01, 0x02]).unwrap();
        // P1 sends 4 bytes (2 chunks)
        sync.process_client_input(1, vec![0x03, 0x04, 0x05, 0x06])
            .unwrap();
        // P2 sends input - creates first bundle
        let outputs = sync.process_client_input(2, vec![0x07, 0x08]).unwrap();

        // P0 (delay 1): needs 1 bundle * 3 players * 2 bytes = 6 bytes
        // P1 (delay 2): needs 2 bundles * 3 players * 2 bytes = 12 bytes
        // P2 (delay 1): needs 6 bytes
        // First bundle: [0x01, 0x02] + [0x03, 0x04] + [0x07, 0x08] = 6 bytes
        // P0 and P2 should output (6 bytes >= 6)
        // P1 should not output yet (6 bytes < 12)

        let p0_outputs: Vec<_> = outputs.iter().filter(|o| o.player_id == 0).collect();
        let p1_outputs: Vec<_> = outputs.iter().filter(|o| o.player_id == 1).collect();
        let p2_outputs: Vec<_> = outputs.iter().filter(|o| o.player_id == 2).collect();

        assert_eq!(p0_outputs.len(), 1);
        assert_eq!(
            p0_outputs[0].output,
            vec![0x01, 0x02, 0x03, 0x04, 0x07, 0x08]
        );
        assert_eq!(p1_outputs.len(), 0);
        assert_eq!(p2_outputs.len(), 1);
        assert_eq!(
            p2_outputs[0].output,
            vec![0x01, 0x02, 0x03, 0x04, 0x07, 0x08]
        );
    }

    #[test]
    fn test_partial_chunk_ignored() {
        let mut sync = SimplestGameSync::new(vec![1, 1]);

        // P0 sends 3 bytes - last byte should be ignored
        sync.process_client_input(0, vec![0x01, 0x02, 0x03])
            .unwrap();
        // P1 sends input - only first 2 bytes from P0 should be used
        let outputs = sync.process_client_input(1, vec![0x04, 0x05]).unwrap();

        assert_eq!(outputs.len(), 2);
        assert_eq!(outputs[0].output, vec![0x01, 0x02, 0x04, 0x05]);
        // 0x03 should not appear in output
        assert!(!outputs[0].output.contains(&0x03));
    }

    #[test]
    fn test_no_output_when_buffer_insufficient() {
        let mut sync = SimplestGameSync::new(vec![2, 2]);

        // Both players send input to create first bundle
        sync.process_client_input(0, vec![0x01, 0x02]).unwrap();
        let outputs = sync.process_client_input(1, vec![0x03, 0x04]).unwrap();

        // Each player needs: delay 2 * 2 players * 2 bytes = 8 bytes
        // But we only have 1 bundle = 4 bytes, so no output
        assert_eq!(outputs.len(), 0);

        // Send second round of inputs
        sync.process_client_input(0, vec![0x05, 0x06]).unwrap();
        let outputs = sync.process_client_input(1, vec![0x07, 0x08]).unwrap();

        // Now we have 2 bundles = 8 bytes, both should output
        assert_eq!(outputs.len(), 2);
        // P0 and P1 should get both bundles combined
        assert_eq!(outputs[0].output.len(), 8);
    }

    #[test]
    fn test_high_delay_scenario() {
        let mut sync = SimplestGameSync::new(vec![3, 3]);

        // Need 3 bundles for output (3 * 2 players * 2 bytes = 12 bytes)
        for round in 0..3 {
            sync.process_client_input(0, vec![0x01 + (round * 2) as u8, 0x02 + (round * 2) as u8])
                .unwrap();
            let outputs = sync
                .process_client_input(1, vec![0x10 + (round * 2) as u8, 0x11 + (round * 2) as u8])
                .unwrap();

            if round < 2 {
                assert_eq!(
                    outputs.len(),
                    0,
                    "Round {} should not produce output",
                    round
                );
            } else {
                assert_eq!(
                    outputs.len(),
                    2,
                    "Round {} should produce output for both",
                    round
                );
                assert_eq!(outputs[0].output.len(), 12);
            }
        }
    }

    #[test]
    fn test_alternating_player_inputs() {
        let mut sync = SimplestGameSync::new(vec![1, 1]);

        // Alternating pattern: P0, P1, P0, P1, ...
        sync.process_client_input(0, vec![0x01, 0x02]).unwrap();
        let outputs = sync.process_client_input(1, vec![0x03, 0x04]).unwrap();
        assert_eq!(outputs.len(), 2);

        sync.process_client_input(0, vec![0x05, 0x06]).unwrap();
        let outputs = sync.process_client_input(1, vec![0x07, 0x08]).unwrap();
        assert_eq!(outputs.len(), 2);
        assert_eq!(outputs[0].output, vec![0x05, 0x06, 0x07, 0x08]);
    }

    // Tests for CachedGameSync wrapper
    #[test]
    fn test_cached_game_sync_equal_delays() {
        let mut sync = CachedGameSync::new(vec![1, 1]);

        // Frame 1: P0 sends GameData
        let outputs = sync
            .process_client_input(0, ClientInput::GameData(vec![0x01, 0x02]))
            .unwrap();
        assert_eq!(outputs.len(), 0);

        // Frame 1: P1 sends GameData
        let outputs = sync
            .process_client_input(1, ClientInput::GameData(vec![0x03, 0x04]))
            .unwrap();
        assert_eq!(outputs.len(), 2);

        // Both should get GameData (first time, not in cache)
        assert_eq!(outputs[0].player_id, 0);
        match &outputs[0].response {
            ServerResponse::GameData(data) => {
                assert_eq!(data, &vec![0x01, 0x02, 0x03, 0x04]);
            }
            _ => panic!("First output should be GameData"),
        }

        assert_eq!(outputs[1].player_id, 1);
        match &outputs[1].response {
            ServerResponse::GameData(data) => {
                assert_eq!(data, &vec![0x01, 0x02, 0x03, 0x04]);
            }
            _ => panic!("First output should be GameData"),
        }
    }

    #[test]
    fn test_cached_game_sync_cache_mechanism() {
        let mut sync = CachedGameSync::new(vec![1, 1]);

        // Frame 1: Both send GameData
        sync.process_client_input(0, ClientInput::GameData(vec![0x00, 0x00]))
            .unwrap();
        let outputs = sync
            .process_client_input(1, ClientInput::GameData(vec![0x00, 0x00]))
            .unwrap();

        assert_eq!(outputs.len(), 2);
        assert!(matches!(outputs[0].response, ServerResponse::GameData(_)));

        // Frame 2: Both send GameCache referencing position 0
        sync.process_client_input(0, ClientInput::GameCache(0))
            .unwrap();
        let outputs = sync
            .process_client_input(1, ClientInput::GameCache(0))
            .unwrap();

        // Should return GameCache since output matches cached data
        assert_eq!(outputs.len(), 2);
        assert!(matches!(outputs[0].response, ServerResponse::GameCache(_)));
        assert!(matches!(outputs[1].response, ServerResponse::GameCache(_)));
    }

    #[test]
    fn test_cached_game_sync_gamecache_input() {
        let mut sync = CachedGameSync::new(vec![1, 1]);

        // First, send GameData to populate cache
        sync.process_client_input(0, ClientInput::GameData(vec![0xAA, 0xBB]))
            .unwrap();
        sync.process_client_input(1, ClientInput::GameData(vec![0xCC, 0xDD]))
            .unwrap();

        // Now use GameCache to reference the cached input
        sync.process_client_input(0, ClientInput::GameCache(0))
            .unwrap();
        let outputs = sync
            .process_client_input(1, ClientInput::GameCache(0))
            .unwrap();

        // Should return GameCache since output [0xAA, 0xBB, 0xCC, 0xDD] was already cached in first round
        assert_eq!(outputs.len(), 2);
        match &outputs[0].response {
            ServerResponse::GameCache(cache_pos) => {
                // Verify it references the cached data from first round
                assert_eq!(*cache_pos, 0);
            }
            _ => panic!("Should return GameCache since same output was cached"),
        }
    }

    #[test]
    fn test_cached_game_sync_different_delays() {
        let mut sync = CachedGameSync::new(vec![1, 2]);

        // P0 sends first input
        let outputs = sync
            .process_client_input(0, ClientInput::GameData(vec![0x01, 0x02]))
            .unwrap();
        assert_eq!(outputs.len(), 0);

        // P1 sends first input (4 bytes)
        let outputs = sync
            .process_client_input(1, ClientInput::GameData(vec![0x03, 0x04, 0x05, 0x06]))
            .unwrap();
        assert_eq!(outputs.len(), 1);
        assert_eq!(outputs[0].player_id, 0);

        // P0 sends second input
        let outputs = sync
            .process_client_input(0, ClientInput::GameData(vec![0x07, 0x08]))
            .unwrap();
        assert_eq!(outputs.len(), 2);
        let p0_output = outputs.iter().find(|o| o.player_id == 0).unwrap();
        let p1_output = outputs.iter().find(|o| o.player_id == 1).unwrap();

        match &p0_output.response {
            ServerResponse::GameData(data) => {
                assert_eq!(data, &vec![0x07, 0x08, 0x05, 0x06]);
            }
            _ => panic!("Should be GameData"),
        }

        match &p1_output.response {
            ServerResponse::GameData(data) => {
                assert_eq!(data.len(), 8);
            }
            _ => panic!("Should be GameData"),
        }
    }

    #[test]
    fn test_cached_game_sync_output_cache_hit() {
        let mut sync = CachedGameSync::new(vec![1, 1]);

        // First round: both players send input
        sync.process_client_input(0, ClientInput::GameData(vec![0x11, 0x22]))
            .unwrap();
        let outputs_first = sync
            .process_client_input(1, ClientInput::GameData(vec![0x33, 0x44]))
            .unwrap();

        // First round output should be GameData (not in cache yet)
        assert_eq!(outputs_first.len(), 2);
        assert!(matches!(
            outputs_first[0].response,
            ServerResponse::GameData(_)
        ));

        // Second round: send same inputs again
        sync.process_client_input(0, ClientInput::GameData(vec![0x11, 0x22]))
            .unwrap();
        let outputs_second = sync
            .process_client_input(1, ClientInput::GameData(vec![0x33, 0x44]))
            .unwrap();

        // Second round: same combination should use cache
        // Output is [0x11, 0x22, 0x33, 0x44] which was sent in first round
        assert_eq!(outputs_second.len(), 2);
        assert!(matches!(
            outputs_second[0].response,
            ServerResponse::GameCache(_)
        ));
    }

    #[test]
    fn test_error_invalid_player_id() {
        let mut sync = SimplestGameSync::new(vec![1, 1]);
        let result = sync.process_client_input(99, vec![0x01, 0x02]);
        assert!(matches!(
            result,
            Err(GameSyncError::InvalidPlayerId { player_id: 99, .. })
        ));

        let mut cached_sync = CachedGameSync::new(vec![1, 1]);
        let result = cached_sync.process_client_input(99, ClientInput::GameData(vec![0x01, 0x02]));
        assert!(matches!(
            result,
            Err(GameSyncError::InvalidPlayerId { player_id: 99, .. })
        ));
    }

    #[test]
    fn test_dropped_player_handling() {
        let mut sync = SimplestGameSync::new(vec![1, 1]);

        // Mark P1 as dropped
        sync.mark_player_dropped(1).unwrap();
        assert!(sync.is_player_dropped(1));
        assert!(!sync.is_player_dropped(0));

        // P0 sends input
        let outputs = sync.process_client_input(0, vec![0x01, 0x02]).unwrap();

        // Should create bundle with P0's input and P1's empty input [0x00, 0x00]
        // Each player needs: delay 1 * 2 players * 2 bytes = 4 bytes
        // Bundle: [0x01, 0x02] + [0x00, 0x00] = 4 bytes, so both should output
        assert_eq!(outputs.len(), 2);

        // Both players should get the same bundle
        for output in &outputs {
            assert_eq!(output.output, vec![0x01, 0x02, 0x00, 0x00]);
        }
    }

    #[test]
    fn test_error_cache_position_not_found() {
        let mut sync = CachedGameSync::new(vec![1, 1]);
        // Try to use cache position that doesn't exist
        let result = sync.process_client_input(0, ClientInput::GameCache(0));
        assert!(matches!(
            result,
            Err(GameSyncError::CachePositionNotFound {
                player_id: 0,
                position: 0
            })
        ));

        // Send GameData first to populate cache
        sync.process_client_input(0, ClientInput::GameData(vec![0x01, 0x02]))
            .unwrap();
        sync.process_client_input(1, ClientInput::GameData(vec![0x03, 0x04]))
            .unwrap();

        // Now try invalid cache position
        let result = sync.process_client_input(0, ClientInput::GameCache(99));
        assert!(matches!(
            result,
            Err(GameSyncError::CachePositionNotFound {
                player_id: 0,
                position: 99
            })
        ));
    }
}
