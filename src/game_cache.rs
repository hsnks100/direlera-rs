#![allow(dead_code)]

use std::collections::{HashMap, VecDeque};
use std::fmt::Debug;
use std::sync::Arc;
use tokio::sync::Mutex;

const CACHE_SIZE: usize = 256;

/// Represents a cache for storing game data packets.
#[derive(Debug, Clone)]
pub struct GameCache {
    /// A deque to hold cached game data.
    cache: VecDeque<Vec<u8>>,
}

impl Default for GameCache {
    fn default() -> Self {
        Self::new()
    }
}

impl GameCache {
    /// Creates a new GameCache with a fixed size.
    pub fn new() -> Self {
        Self {
            cache: VecDeque::with_capacity(CACHE_SIZE),
        }
    }

    /// Searches for the game data in the cache.
    /// Returns `Some(position)` if found, otherwise `None`.
    pub fn find(&self, data: &Vec<u8>) -> Option<u8> {
        self.cache
            .iter()
            .position(|cached_data| cached_data == data)
            .map(|pos| pos as u8)
    }

    /// Adds new game data to the cache.
    /// If the cache is full, it removes the oldest entry.
    /// Returns the position where the data was added.
    pub fn add(&mut self, data: Vec<u8>) -> u8 {
        if self.cache.len() == CACHE_SIZE {
            self.cache.pop_front();
        }
        self.cache.push_back(data);
        (self.cache.len() - 1) as u8
    }

    /// Retrieves game data from the cache by position.
    #[allow(dead_code)]
    pub fn get(&self, position: u8) -> Option<&Vec<u8>> {
        self.cache.get(position as usize)
    }
}

/// Trait representing a connected client.
#[allow(dead_code)]
pub trait ClientTrait: Debug {
    fn id(&self) -> u32;

    /// Gets the receive cache of the client.
    fn get_receive_cache(&mut self) -> &mut GameCache;

    /// Adds an input to the pending inputs queue.
    fn add_input(&mut self, data: Vec<u8>);

    /// Checks if the client has pending inputs.
    fn has_pending_input(&self) -> bool;

    /// Retrieves and removes the next input from the pending queue.
    fn get_next_input(&mut self) -> Option<Vec<u8>>;

    /// Handles incoming data from the client.
    /// Stores the data in the client's pending inputs.
    fn handle_incoming(&mut self, data: Vec<u8>) {
        let actual_data = if data.len() == 1 {
            let position = data[0];
            if let Some(cached_data) = self.get_receive_cache().get(position) {
                cached_data.clone()
            } else {
                panic!(
                    "Invalid cache position {} received from client {}",
                    position,
                    self.id()
                );
            }
        } else {
            // Add data to client's receive cache.
            let cache = self.get_receive_cache();
            cache.add(data.clone());
            data
        };

        self.add_input(actual_data);
    }
}

/// Represents the result of processing a frame.
#[allow(dead_code)]
pub struct FrameResult {
    pub frame_index: usize,
    pub use_cache: bool,
    pub cache_pos: u8,
    pub data_to_send: Vec<u8>,
    pub aggregated_data: Vec<u8>,
}

/// Represents the game data processor for handling inputs and outputs.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct GameDataProcessor {
    /// Map of connected clients.
    clients: HashMap<u32, Arc<Mutex<dyn ClientTrait>>>,
    /// Global cache for aggregated game data.
    aggregated_cache: GameCache,
    /// Current frame index.
    frame_index: usize,
    /// Stores collected inputs per frame.
    frame_inputs: HashMap<usize, HashMap<u32, Vec<u8>>>,
}

impl Default for GameDataProcessor {
    fn default() -> Self {
        Self::new()
    }
}

impl GameDataProcessor {
    #[allow(dead_code)]
    /// Creates a new GameDataProcessor with no clients.
    pub fn new() -> Self {
        Self {
            clients: HashMap::new(),
            aggregated_cache: GameCache::new(),
            frame_index: 0,
            frame_inputs: HashMap::new(),
        }
    }

    #[allow(dead_code)]
    /// Adds a new client to the processor.
    pub async fn add_client(&mut self, client: Arc<Mutex<dyn ClientTrait>>) {
        let client_id = client.lock().await.id();
        self.clients.insert(client_id, client);
    }

    /// Processes incoming game data from a client.
    /// Stores the data in the client's pending inputs.
    pub async fn process_incoming(&mut self, client_id: u32, data: Vec<u8>) {
        let client = self.clients.get(&client_id).expect("Client not found");
        let mut client = client.lock().await;
        client.handle_incoming(data);
    }

    /// Processes a frame if inputs from all clients are available.
    /// Returns Some(FrameResult) if the frame was processed.
    pub async fn process_frame(&mut self) -> Option<FrameResult> {
        let mut frame_data = Vec::new();

        let all_clients_have_input = {
            let mut all_have_input = true;
            for client in self.clients.values() {
                let client = client.lock().await;
                if !client.has_pending_input() {
                    all_have_input = false;
                    break;
                }
            }
            all_have_input
        };

        if !all_clients_have_input {
            return None;
        }

        for (&client_id, client) in &self.clients {
            let mut client = client.lock().await;
            let input = client.get_next_input().expect("Input should be available");
            frame_data.extend(input.clone());
            self.frame_inputs
                .entry(self.frame_index)
                .or_default()
                .insert(client_id, input);
        }

        let (use_cache, cache_pos, data_to_send) = self.prepare_outgoing(frame_data.clone());

        let frame_result = FrameResult {
            frame_index: self.frame_index,
            use_cache,
            cache_pos,
            data_to_send,
            aggregated_data: frame_data,
        };

        self.frame_index += 1;
        Some(frame_result)
    }

    #[allow(dead_code)]
    /// Prepares outgoing game data to send to clients.
    pub fn prepare_outgoing(&mut self, data: Vec<u8>) -> (bool, u8, Vec<u8>) {
        if let Some(pos) = self.aggregated_cache.find(&data) {
            (true, pos, Vec::new())
        } else {
            let pos = self.aggregated_cache.add(data.clone());
            (false, pos, data)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Mock client for testing purposes.
    #[derive(Debug)]
    struct MockClient {
        id: u32,
        pending_inputs: VecDeque<Vec<u8>>,
        receive_cache: crate::game_cache::GameCache,
    }

    impl MockClient {
        fn new(id: u32) -> Self {
            Self {
                id,
                pending_inputs: VecDeque::new(),
                receive_cache: crate::game_cache::GameCache::new(),
            }
        }
    }

    impl ClientTrait for MockClient {
        fn id(&self) -> u32 {
            self.id
        }

        fn get_receive_cache(&mut self) -> &mut GameCache {
            &mut self.receive_cache
        }

        fn add_input(&mut self, data: Vec<u8>) {
            self.pending_inputs.push_back(data);
        }

        fn has_pending_input(&self) -> bool {
            !self.pending_inputs.is_empty()
        }

        fn get_next_input(&mut self) -> Option<Vec<u8>> {
            self.pending_inputs.pop_front()
        }
    }

    #[tokio::test]
    async fn test_game_data_processing_async() {
        let mut processor = GameDataProcessor::new();

        // Simulate two clients connecting.
        processor
            .add_client(Arc::new(Mutex::new(MockClient::new(1))))
            .await;
        processor
            .add_client(Arc::new(Mutex::new(MockClient::new(2))))
            .await;

        // Expected cache usage and positions per frame.
        let expected_results = [
            (false, 0), // Frame 1: New data, cache position 0
            (true, 0),  // Frame 2: Using cache position 0
            (false, 1), // Frame 3: New data, cache position 1
            (false, 2), // Frame 4: New data, cache position 2
            (true, 1),
        ];

        // Test inputs from clients at different times.
        let inputs = vec![
            (1, vec![1, 2, 3, 4, 5, 6]),       // Client 1 sends input for Frame 1
            (1, vec![0]),                      // Client 1 sends cache position for Frame 2
            (2, vec![10, 11, 12, 13, 14, 15]), // Client 2 sends input for Frame 1
            (2, vec![0]),                      // Client 2 sends cache position for Frame 2
            (1, vec![20, 21, 22, 23, 24, 25]), // Client 1 sends new input for Frame 3
            (2, vec![0]),                      // Client 2 sends cache position for Frame 3
            (2, vec![26, 27, 28, 29, 30, 31]), // Client 2 sends new input for Frame 4
            (1, vec![0]),                      // Client 1 sends cache position for Frame 4
            (1, vec![20, 21, 22, 23, 24, 25]), // Client 1 sends cache position for Frame 5
            (2, vec![0]),                      // Client 2 sends cache position for Frame 5
        ];

        // Expected frames to be processed.
        let expected_frames = expected_results.len();
        let mut frames_processed = 0;
        let mut frame_results = Vec::new();

        for (client_id, input) in inputs {
            processor.process_incoming(client_id, input).await;

            // Attempt to process frames as inputs become available.
            while let Some(frame_result) = processor.process_frame().await {
                frames_processed += 1;
                frame_results.push(frame_result);
            }
        }

        assert_eq!(
            frames_processed, expected_frames,
            "All frames should be processed"
        );

        // Perform assertions on the frame results.
        for (i, frame_result) in frame_results.iter().enumerate() {
            let (exp_use_cache, exp_cache_pos) = expected_results[i];
            println!(
                "send to bytes: {}: {:?}",
                frame_result.use_cache, frame_result.data_to_send
            );
            assert_eq!(
                frame_result.use_cache,
                exp_use_cache,
                "Frame {}: Expected use_cache to be {}",
                frame_result.frame_index + 1,
                exp_use_cache
            );
            assert_eq!(
                frame_result.cache_pos,
                exp_cache_pos,
                "Frame {}: Expected cache_pos to be {}",
                frame_result.frame_index + 1,
                exp_cache_pos
            );
        }
    }
}
