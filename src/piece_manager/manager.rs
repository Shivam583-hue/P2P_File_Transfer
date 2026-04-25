#[derive(PartialEq, Clone)]
pub enum ChunkState {
    Needed,
    InFlight,
    Done,
}

pub struct PieceManager {
    pub states: Vec<ChunkState>,
}

impl PieceManager {
    pub fn next_chunk(&mut self) -> Option<usize> {
        for i in 0..self.states.len() {
            if self.states[i] == ChunkState::Needed {
                self.states[i] = ChunkState::InFlight;
                return Some(i);
            }
        }
        None
    }

    pub fn complete(&mut self, i: usize) {
        self.states[i] = ChunkState::Done;
    }

    pub fn requeue(&mut self, i: usize) {
        self.states[i] = ChunkState::Needed;
    }

    pub fn is_done(&self) -> bool {
        self.states.iter().all(|s| *s == ChunkState::Done)
    }
}
