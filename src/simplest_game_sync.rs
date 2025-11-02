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

use crate::simple_game_sync::InputCache;

// Re-export for convenience
pub use crate::simple_game_sync::{ClientInput, ServerResponse};

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
        let mut results = Vec::new();
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

            // Check for outputs after each bundle creation
            // This ensures players with smaller delays get outputs immediately
            for player_id in 0..self.player_delays.len() {
                let required_size =
                    self.player_delays[player_id] * self.player_delays.len() * MINIMUM_INPUT_SIZE;
                while self.output_buffers[player_id].len() >= required_size {
                    let output = self.output_buffers[player_id]
                        .drain(..required_size)
                        .collect::<Vec<u8>>();
                    results.push(PlayerOutput { player_id, output });
                }
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
        let mut manager = CachedGameSync::new(vec![1, 1]);

        // Frame 1: P0 sends input
        let outputs = manager
            .process_client_input(0, ClientInput::GameData(vec![0x01, 0x02]))
            .unwrap();
        assert_eq!(outputs.len(), 0);

        // Frame 1: P1 sends input
        let outputs = manager
            .process_client_input(1, ClientInput::GameData(vec![0x03, 0x04]))
            .unwrap();
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
        manager
            .process_client_input(0, ClientInput::GameCache(0))
            .unwrap();
        let outputs = manager
            .process_client_input(1, ClientInput::GameCache(0))
            .unwrap();

        assert_eq!(outputs.len(), 2);
        assert!(matches!(outputs[0].response, ServerResponse::GameCache(_)));
        assert!(matches!(outputs[1].response, ServerResponse::GameCache(_)));
    }

    #[test]
    fn test_different_delays() {
        let mut manager = CachedGameSync::new(vec![1, 2]);

        // P0 sends first input
        let outputs = manager
            .process_client_input(0, ClientInput::GameData(vec![0x01, 0x00]))
            .unwrap();
        assert_eq!(outputs.len(), 0);

        // P0 sends second and third
        manager
            .process_client_input(0, ClientInput::GameData(vec![0x02, 0x00]))
            .unwrap();
        let outputs = manager
            .process_client_input(0, ClientInput::GameData(vec![0x03, 0x00]))
            .unwrap();
        assert_eq!(outputs.len(), 0);

        // P1 sends 4 bytes (2 frames)
        // When P1 sends input, bundles are created from accumulated P0 frames
        // P0 needs: delay 1 * 2 players * 2 bytes = 4 bytes
        // P1 needs: delay 2 * 2 players * 2 bytes = 8 bytes
        // Bundle size: 2 players * 2 bytes = 4 bytes per bundle
        let outputs = manager
            .process_client_input(1, ClientInput::GameData(vec![0xAA, 0xBB, 0xCC, 0xDD]))
            .unwrap();

        // Check that we got outputs
        // With the fixed logic:
        // - Bundle 1: [0x01, 0x00] + [0xAA, 0xBB] → P0 gets output immediately (4 bytes >= 4)
        // - Bundle 2: [0x02, 0x00] + [0xCC, 0xDD] → P0 gets another output (4 bytes >= 4), P1 gets output (8 bytes >= 8)
        let p0_outputs: Vec<_> = outputs.iter().filter(|o| o.player_id == 0).collect();
        let p1_outputs: Vec<_> = outputs.iter().filter(|o| o.player_id == 1).collect();

        // P0 should get 2 outputs (one after each bundle, since delay 1 needs 4 bytes = 1 bundle)
        assert_eq!(
            p0_outputs.len(),
            2,
            "P0 should get 2 outputs (one per bundle)"
        );

        // Verify P0's first output (after Bundle 1)
        match &p0_outputs[0].response {
            ServerResponse::GameData(data) => {
                assert_eq!(
                    data,
                    &vec![0x01, 0x00, 0xAA, 0xBB],
                    "P0's first output should be Bundle 1"
                );
            }
            _ => panic!("P0's first output should be GameData"),
        }

        // Verify P0's second output (after Bundle 2)
        match &p0_outputs[1].response {
            ServerResponse::GameData(data) => {
                assert_eq!(
                    data,
                    &vec![0x02, 0x00, 0xCC, 0xDD],
                    "P0's second output should be Bundle 2"
                );
            }
            _ => panic!("P0's second output should be GameData"),
        }

        // P1 should get 1 output (after 2 bundles, since delay 2 needs 8 bytes = 2 bundles)
        assert_eq!(
            p1_outputs.len(),
            1,
            "P1 should get 1 output (after 2 bundles)"
        );

        // Verify P1's output (after Bundle 2, contains both bundles)
        match &p1_outputs[0].response {
            ServerResponse::GameData(data) => {
                assert_eq!(
                    data,
                    &vec![0x01, 0x00, 0xAA, 0xBB, 0x02, 0x00, 0xCC, 0xDD],
                    "P1's output should contain both bundles"
                );
            }
            _ => panic!("P1's output should be GameData"),
        }
    }

    #[test]
    fn test_cache_mechanism() {
        let mut manager = CachedGameSync::new(vec![1, 1]);

        manager
            .process_client_input(0, ClientInput::GameData(vec![0x00, 0x00]))
            .unwrap();
        let outputs = manager
            .process_client_input(1, ClientInput::GameData(vec![0x00, 0x00]))
            .unwrap();

        assert_eq!(outputs.len(), 2);
        assert!(matches!(outputs[0].response, ServerResponse::GameData(_)));

        manager
            .process_client_input(0, ClientInput::GameCache(0))
            .unwrap();
        let outputs = manager
            .process_client_input(1, ClientInput::GameCache(0))
            .unwrap();

        let has_cache = outputs
            .iter()
            .any(|o| matches!(o.response, ServerResponse::GameCache(_)));
        assert!(has_cache);
    }

    #[test]
    fn test_three_players() {
        let mut manager = CachedGameSync::new(vec![1, 1, 2]);

        manager
            .process_client_input(0, ClientInput::GameData(vec![0x01, 0x00]))
            .unwrap();
        let outputs = manager
            .process_client_input(1, ClientInput::GameData(vec![0x02, 0x00]))
            .unwrap();

        assert_eq!(outputs.len(), 0); // P2 hasn't sent input yet

        // P2 sends input
        let outputs = manager
            .process_client_input(2, ClientInput::GameData(vec![0x03, 0x00, 0x04, 0x00]))
            .unwrap();

        assert!(outputs.len() >= 2);
        let p0_outputs: Vec<_> = outputs.iter().filter(|o| o.player_id == 0).collect();
        let p1_outputs: Vec<_> = outputs.iter().filter(|o| o.player_id == 1).collect();

        assert_eq!(p0_outputs.len(), 1);
        assert_eq!(p1_outputs.len(), 1);
    }

    #[test]
    fn test_gd_gc_pattern_delay_1() {
        let mut manager = CachedGameSync::new(vec![1, 1]);

        // Frame 1
        manager
            .process_client_input(0, ClientInput::GameData(vec![0xAA, 0xBB]))
            .unwrap();
        let outputs = manager
            .process_client_input(1, ClientInput::GameData(vec![0xCC, 0xDD]))
            .unwrap();
        assert_eq!(outputs.len(), 2);

        // Frame 2
        manager
            .process_client_input(0, ClientInput::GameCache(0))
            .unwrap();
        let outputs = manager
            .process_client_input(1, ClientInput::GameCache(0))
            .unwrap();
        assert!(matches!(outputs[0].response, ServerResponse::GameCache(_)));

        // Frame 3
        manager
            .process_client_input(0, ClientInput::GameCache(0))
            .unwrap();
        let outputs = manager
            .process_client_input(1, ClientInput::GameCache(0))
            .unwrap();
        assert!(matches!(outputs[0].response, ServerResponse::GameCache(_)));

        // Frame 4
        manager
            .process_client_input(0, ClientInput::GameData(vec![0x11, 0x22]))
            .unwrap();
        let outputs = manager
            .process_client_input(1, ClientInput::GameData(vec![0x33, 0x44]))
            .unwrap();
        assert!(matches!(outputs[0].response, ServerResponse::GameData(_)));
    }

    #[test]
    fn test_gd_gc_pattern_delay_2() {
        let mut manager = CachedGameSync::new(vec![2, 2]);

        manager
            .process_client_input(0, ClientInput::GameData(vec![0xAA, 0xBB, 0xAA, 0xBB]))
            .unwrap();
        let outputs = manager
            .process_client_input(1, ClientInput::GameData(vec![0xCC, 0xDD, 0xCC, 0xDD]))
            .unwrap();
        assert_eq!(outputs.len(), 2);

        manager
            .process_client_input(0, ClientInput::GameCache(0))
            .unwrap();
        let outputs = manager
            .process_client_input(1, ClientInput::GameCache(0))
            .unwrap();
        assert!(matches!(outputs[0].response, ServerResponse::GameCache(_)));
    }

    #[test]
    fn test_gd_gc_creates_new_combined() {
        let mut manager = CachedGameSync::new(vec![1, 1]);

        manager
            .process_client_input(0, ClientInput::GameData(vec![0x01, 0x02]))
            .unwrap();
        manager
            .process_client_input(1, ClientInput::GameData(vec![0x03, 0x04]))
            .unwrap();

        manager
            .process_client_input(0, ClientInput::GameData(vec![0x05, 0x06]))
            .unwrap();
        let outputs = manager
            .process_client_input(1, ClientInput::GameCache(0))
            .unwrap();

        match &outputs[0].response {
            ServerResponse::GameData(data) => {
                assert_eq!(data, &vec![0x05, 0x06, 0x03, 0x04]);
            }
            _ => panic!("Should be GameData"),
        }
    }

    #[test]
    fn test_gc_gc_creates_new_combined() {
        let mut manager = CachedGameSync::new(vec![1, 1]);

        manager
            .process_client_input(0, ClientInput::GameData(vec![0x01, 0x02]))
            .unwrap();
        manager
            .process_client_input(1, ClientInput::GameData(vec![0x03, 0x04]))
            .unwrap();

        manager
            .process_client_input(0, ClientInput::GameCache(0))
            .unwrap();
        manager
            .process_client_input(1, ClientInput::GameData(vec![0x05, 0x06]))
            .unwrap();

        manager
            .process_client_input(0, ClientInput::GameData(vec![0x07, 0x08]))
            .unwrap();
        manager
            .process_client_input(1, ClientInput::GameCache(0))
            .unwrap();

        manager
            .process_client_input(0, ClientInput::GameCache(0))
            .unwrap();
        let outputs = manager
            .process_client_input(1, ClientInput::GameCache(1))
            .unwrap();
        assert!(matches!(outputs[0].response, ServerResponse::GameCache(_)));

        manager
            .process_client_input(0, ClientInput::GameCache(1))
            .unwrap();
        let outputs = manager
            .process_client_input(1, ClientInput::GameCache(1))
            .unwrap();
        match &outputs[0].response {
            ServerResponse::GameData(_) => {}
            _ => panic!("Should be GameData (new combination)"),
        }
    }

    #[test]
    fn test_delay_2_with_4_bytes_succeeds() {
        let mut manager = CachedGameSync::new(vec![2, 2]);

        manager
            .process_client_input(0, ClientInput::GameData(vec![0xAA, 0xBB, 0xCC, 0xDD]))
            .unwrap();
        let outputs = manager
            .process_client_input(1, ClientInput::GameData(vec![0x11, 0x22, 0x33, 0x44]))
            .unwrap();

        assert_eq!(outputs.len(), 2);
    }

    #[test]
    fn test_invalid_player_id() {
        let mut manager = CachedGameSync::new(vec![1, 1]);
        let result = manager.process_client_input(2, ClientInput::GameData(vec![0x01, 0x02]));

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            GameSyncError::InvalidPlayerId { .. }
        ));
    }

    #[test]
    fn test_cache_position_not_found() {
        let mut manager = CachedGameSync::new(vec![1, 1]);
        let result = manager.process_client_input(0, ClientInput::GameCache(0));

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            GameSyncError::CachePositionNotFound { .. }
        ));
    }

    #[test]
    fn test_player_delay() {
        let manager = CachedGameSync::new(vec![1, 2, 3]);
        assert_eq!(manager.get_player_delay(0), 1);
        assert_eq!(manager.get_player_delay(1), 2);
        assert_eq!(manager.get_player_delay(2), 3);
    }

    #[test]
    fn test_player_count() {
        let manager = CachedGameSync::new(vec![1, 1, 1]);
        assert_eq!(manager.player_count(), 3);
    }
}
