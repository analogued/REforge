// Copyright (c) 2026 analogued. Project license has not been selected.
//! Encounter commands and graph edges decoded from external game resources.

use crate::Error;

const SCRIPT_COUNT: usize = 21;
const SCRIPT_LENGTH: usize = 50;
const SCRIPT_OFFSET: usize = 0x0a3e;
const EDGE_OFFSET: usize = 0x0e58;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScriptCommand {
    Terrain(u16),
    Special(u8),
    Enemy(u8),
    Control(u8),
}

impl ScriptCommand {
    pub fn decode(byte: u8) -> Self {
        match byte & 0xc0 {
            0x00 => Self::Terrain(u16::from(byte)),
            0x40 => Self::Special(byte),
            0x80 => Self::Enemy(byte),
            _ => Self::Control(byte),
        }
    }
}

#[derive(Clone, Debug)]
pub struct ScriptRules {
    scripts: Vec<[u8; SCRIPT_LENGTH]>,
    edges: Vec<[u16; 2]>,
    data_segment: Vec<u8>,
}

impl ScriptRules {
    /// HUD marker family and placement share difficulty-indexed external data.
    pub fn marker_styles(&self, difficulty: u16) -> Result<[(usize, i16, i16); 3], Error> {
        let sources = self.patrol_sources(difficulty)?;
        let read = |offset: u16| -> Result<i16, Error> {
            let low = *self.data_segment.get(usize::from(offset)).ok_or_else(|| {
                Error::InvalidData("marker table outside supplied data segment".into())
            })?;
            let high = *self
                .data_segment
                .get(usize::from(offset.wrapping_add(1)))
                .ok_or_else(|| {
                    Error::InvalidData("marker table outside supplied data segment".into())
                })?;
            Ok(i16::from_le_bytes([low, high]))
        };
        let mut styles = [(0, 0, 0); 3];
        for choice in 0..3 {
            let delta = difficulty.wrapping_mul(6).wrapping_add(choice as u16 * 2);
            styles[choice] = (
                sources[choice],
                38_i16.wrapping_sub(read(0x04ca_u16.wrapping_add(delta))?),
                4_i16.wrapping_add(read(0x0506_u16.wrapping_add(delta))?),
            );
        }
        Ok(styles)
    }

    pub fn data_segment(&self) -> &[u8] {
        &self.data_segment
    }

    /// `data` starts at the expanded program's data segment. The caller owns
    /// decompression and loading; this module contains no original script data.
    pub fn from_data_segment(data: &[u8]) -> Result<Self, Error> {
        if data.len() < EDGE_OFFSET + SCRIPT_COUNT * 4 {
            return Err(Error::InvalidData("truncated encounter rules".into()));
        }
        let mut scripts = Vec::with_capacity(SCRIPT_COUNT);
        let mut edges = Vec::with_capacity(SCRIPT_COUNT);
        for index in 0..SCRIPT_COUNT {
            let offset = SCRIPT_OFFSET + index * SCRIPT_LENGTH;
            let row: [u8; SCRIPT_LENGTH] = data[offset..offset + SCRIPT_LENGTH]
                .try_into()
                .expect("validated script region");
            if !row.contains(&0xcf) {
                return Err(Error::InvalidData(format!(
                    "encounter script {index} has no progression command"
                )));
            }
            scripts.push(row);
            let offset = EDGE_OFFSET + index * 4;
            let edge = std::array::from_fn(|choice| {
                let at = offset + choice * 2;
                u16::from_le_bytes([data[at], data[at + 1]])
            });
            if edge
                .iter()
                .any(|target| usize::from(*target) >= SCRIPT_COUNT)
            {
                return Err(Error::InvalidData(format!(
                    "encounter script {index} has an invalid progression edge"
                )));
            }
            edges.push(edge);
        }
        Ok(Self {
            scripts,
            edges,
            data_segment: data.to_vec(),
        })
    }

    pub fn next(&self, script: u16, cursor: u16) -> Result<ScriptCommand, Error> {
        let byte = self
            .scripts
            .get(usize::from(script))
            .and_then(|row| row.get(usize::from(cursor)))
            .ok_or_else(|| Error::InvalidData("encounter cursor outside script rules".into()))?;
        Ok(ScriptCommand::decode(*byte))
    }

    /// A normal script ending follows edge zero; entering a special goal follows
    /// edge one and clears the remaining primary and special objects immediately.
    pub fn progression(&self, script: u16, goal_entered: bool) -> Result<u16, Error> {
        self.edges
            .get(usize::from(script))
            .map(|edge| edge[usize::from(goal_entered)])
            .ok_or_else(|| Error::InvalidData("invalid encounter script index".into()))
    }

    /// Difficulty is a word and remains unclamped. Later indices can read past
    /// the initial marker families into the subsequent external program data.
    pub fn patrol_sources(&self, difficulty: u16) -> Result<[usize; 3], Error> {
        let start = 0x04ac_u16.wrapping_add(difficulty.wrapping_mul(3));
        let mut sources = [0; 3];
        for (choice, source) in sources.iter_mut().enumerate() {
            let offset = start.wrapping_add(choice as u16);
            *source =
                usize::from(*self.data_segment.get(usize::from(offset)).ok_or_else(|| {
                    Error::InvalidData("patrol table outside supplied data segment".into())
                })?);
        }
        Ok(sources)
    }
}

/// The original uses a signed comparison after wrapping word arithmetic.
pub(crate) fn delay_threshold(difficulty: u16) -> i16 {
    20_u16.wrapping_sub(difficulty.wrapping_shl(3)) as i16
}

#[cfg(test)]
mod tests {
    use super::*;

    fn synthetic_data() -> Vec<u8> {
        let mut data = vec![0; EDGE_OFFSET + SCRIPT_COUNT * 4];
        for index in 0..SCRIPT_COUNT {
            let offset = SCRIPT_OFFSET + SCRIPT_LENGTH * index;
            data[offset..offset + 5].copy_from_slice(&[1, 0x41, 0x85, 0xc5, 0xcf]);
            let offset = EDGE_OFFSET + index * 4;
            data[offset..offset + 2].copy_from_slice(&(((index + 1) % 21) as u16).to_le_bytes());
            data[offset + 2..offset + 4].copy_from_slice(&(index as u16).to_le_bytes());
        }
        data
    }

    #[test]
    fn external_commands_and_both_edges_are_decoded() {
        let rules = ScriptRules::from_data_segment(&synthetic_data()).unwrap();
        assert_eq!(rules.next(20, 0).unwrap(), ScriptCommand::Terrain(1));
        assert_eq!(rules.next(20, 1).unwrap(), ScriptCommand::Special(0x41));
        assert_eq!(rules.next(20, 2).unwrap(), ScriptCommand::Enemy(0x85));
        assert_eq!(rules.next(20, 3).unwrap(), ScriptCommand::Control(0xc5));
        assert_eq!(rules.progression(20, false).unwrap(), 0);
        assert_eq!(rules.progression(20, true).unwrap(), 20);
        assert!(rules.next(21, 0).is_err());
        assert!(rules.next(0, 50).is_err());
    }

    #[test]
    fn truncated_or_invalid_graphs_are_rejected() {
        let mut data = synthetic_data();
        assert!(ScriptRules::from_data_segment(&data[..EDGE_OFFSET]).is_err());
        data[EDGE_OFFSET] = 21;
        assert!(ScriptRules::from_data_segment(&data).is_err());
        data = synthetic_data();
        data[SCRIPT_OFFSET + 4] = 0;
        assert!(ScriptRules::from_data_segment(&data).is_err());
    }

    #[test]
    fn event_delay_preserves_signed_and_word_wraparound() {
        assert_eq!(delay_threshold(0), 20);
        assert_eq!(delay_threshold(1), 12);
        assert_eq!(delay_threshold(2), 4);
        assert_eq!(delay_threshold(3), -4);
        assert_eq!(delay_threshold(8192), 20);
        assert_eq!(delay_threshold(u16::MAX), 28);
    }

    #[test]
    fn patrol_lookup_spills_past_nominal_families_and_wraps_word_addresses() {
        let mut data = synthetic_data();
        data[0x4ac + 30..0x4ac + 33].copy_from_slice(&[7, 8, 9]);
        data[0x4a9..0x4ac].copy_from_slice(&[10, 11, 12]);
        let rules = ScriptRules::from_data_segment(&data).unwrap();
        assert_eq!(rules.patrol_sources(10).unwrap(), [7, 8, 9]);
        assert_eq!(rules.patrol_sources(u16::MAX).unwrap(), [10, 11, 12]);
    }
}
