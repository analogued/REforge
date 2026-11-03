// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Release-specific playback edits, separate from the wordcode interpreter.

use sha2::{Digest, Sha256};

struct Edit {
    offset: usize,
    before: &'static [u8],
    after: &'static [u8],
}

struct Patch<'a> {
    size: usize,
    sha256: [u8; 32],
    edits: &'a [Edit],
}

impl Patch<'_> {
    fn apply(&self, bytes: &mut [u8]) -> bool {
        if bytes.len() != self.size || Sha256::digest(&*bytes).as_slice() != self.sha256 {
            return false;
        }
        // Validate every edit before changing any bytes.
        for edit in self.edits {
            if edit.before.len() != edit.after.len() {
                return false;
            }
            let Some(end) = edit.offset.checked_add(edit.before.len()) else {
                return false;
            };
            if bytes.get(edit.offset..end) != Some(edit.before) {
                return false;
            }
        }
        for edit in self.edits {
            bytes[edit.offset..edit.offset + edit.after.len()].copy_from_slice(edit.after);
        }
        true
    }
}

// Identified Mindwheel DOS OBJ only: replace its entry's load0 with a jump to
// the existing opening-scene initialization. Keep the title and story setup.
const MINDWHEEL_DOS: Patch = Patch {
    size: 188_928,
    sha256: [
        0x45, 0x72, 0xf6, 0xfa, 0xe6, 0x8f, 0xa8, 0x1b, 0xe3, 0xe7, 0x23, 0xc0, 0xa4, 0xa9, 0x09,
        0x94, 0xfd, 0x2c, 0x9f, 0x12, 0x19, 0x63, 0x6a, 0x33, 0x4d, 0x2d, 0xb0, 0x76, 0x1d, 0xb0,
        0xb0, 0x91,
    ],
    edits: &[Edit {
        offset: 0x570c,
        before: &[0x2c, 0x00, 0x00, 0x00],
        after: &[0x08, 0x00, 0x62, 0x49],
    }],
};

// Identified Essex DOS OBJ only: keep the title and enter the existing
// opening-scene initialization.
const ESSEX_DOS: Patch = Patch {
    size: 180_736,
    sha256: [
        0x06, 0x57, 0x99, 0x88, 0xcb, 0xfd, 0x31, 0x34, 0x2d, 0x74, 0x1c, 0xbb, 0x72, 0x98, 0x98,
        0xae, 0xa5, 0x75, 0xd9, 0x5f, 0xbd, 0x45, 0x58, 0xc0, 0x9e, 0x09, 0x82, 0xd9, 0x2a, 0xbb,
        0x65, 0x41,
    ],
    edits: &[Edit {
        offset: 0xd314,
        before: &[0x2c, 0x00, 0x00, 0x00],
        after: &[0x08, 0x00, 0x5c, 0xc3],
    }],
};

// Identified Brimstone DOS OBJ only: keep the original entry and opening
// initialization, including its story timing setup.
const BRIMSTONE_DOS: Patch = Patch {
    size: 183_296,
    sha256: [
        0x34, 0xf6, 0xcd, 0x14, 0xb2, 0xf0, 0xd6, 0xfd, 0x1d, 0x5b, 0x8d, 0x6c, 0x99, 0x89, 0x1f,
        0xb6, 0x03, 0xae, 0x4c, 0xf6, 0x92, 0x59, 0x9e, 0x03, 0x9c, 0x8e, 0xec, 0xff, 0x1c, 0x3b,
        0xae, 0xf4,
    ],
    edits: &[
        Edit {
            offset: 0x534a,
            before: &[0x58, 0x00, 0x77, 0x86],
            after: &[0x08, 0x00, 0x52, 0x41],
        },
        Edit {
            offset: 0x5380,
            before: &[0x58, 0x00],
            after: &[0x03, 0x00],
        },
    ],
};

pub(super) fn prepare_for_play(bytes: &mut [u8]) {
    MINDWHEEL_DOS.apply(bytes);
    ESSEX_DOS.apply(bytes);
    BRIMSTONE_DOS.apply(bytes);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ProgramAddr, decode::decode};

    fn authored_patch(bytes: &[u8]) -> Patch<'static> {
        Patch {
            size: bytes.len(),
            sha256: Sha256::digest(bytes).into(),
            edits: &[Edit {
                offset: 2,
                before: &[0x2c, 0, 0, 0],
                after: &[0x08, 0, 0x20, 0],
            }],
        }
    }

    #[test]
    fn identified_patch_changes_only_its_guarded_instruction_and_is_idempotent() {
        let original = [0xaa, 0xbb, 0x2c, 0, 0, 0, 0xcc, 0xdd];
        let mut bytes = original;
        let patch = authored_patch(&original);
        assert!(patch.apply(&mut bytes));
        assert_eq!(bytes, [0xaa, 0xbb, 0x08, 0, 0x20, 0, 0xcc, 0xdd]);
        let instruction = decode(&bytes, ProgramAddr(2)).unwrap();
        assert_eq!(instruction.opcode, 2);
        assert_eq!(instruction.operands[0].raw, 0x20);
        let once = bytes;
        assert!(!patch.apply(&mut bytes));
        assert_eq!(bytes, once);
    }

    #[test]
    fn a_matching_instruction_in_another_release_is_not_enough() {
        let original = [0xaa, 0xbb, 0x2c, 0, 0, 0, 0xcc, 0xdd];
        let patch = authored_patch(&original);
        let mut different = original;
        different[7] ^= 1;
        let before = different;
        assert!(!patch.apply(&mut different));
        assert_eq!(different, before);
        prepare_for_play(&mut different);
        assert_eq!(different, before);
    }

    #[test]
    fn malformed_patch_guards_do_not_partially_change_the_input() {
        let original = [0xaa, 0xbb, 0x2c, 0, 0, 0, 0xcc, 0xdd];
        for (offset, before, after) in [
            (2, &[0, 0, 0, 0][..], &[0x08, 0, 0x20, 0][..]),
            (6, &[0x2c, 0, 0, 0][..], &[0x08, 0, 0x20, 0][..]),
            (usize::MAX, &[0x2c, 0, 0, 0][..], &[0x08, 0, 0x20, 0][..]),
            (2, &[0x2c, 0, 0, 0][..], &[0x08, 0][..]),
        ] {
            let mut bytes = original;
            let mut patch = authored_patch(&original);
            let edits = [
                Edit {
                    offset: 2,
                    before: &[0x2c, 0, 0, 0],
                    after: &[0x08, 0, 0x20, 0],
                },
                Edit {
                    offset,
                    before,
                    after,
                },
            ];
            patch.edits = &edits;
            assert!(!patch.apply(&mut bytes));
            assert_eq!(bytes, original);
        }
        let mut truncated = original[..6].to_vec();
        let before = truncated.clone();
        assert!(!authored_patch(&original).apply(&mut truncated));
        assert_eq!(truncated, before);
    }
}
