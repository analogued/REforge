// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Release-specific playback edits, separate from the wordcode interpreter.

struct Edit {
    offset: usize,
    before: &'static [u8],
    after: &'static [u8],
}

struct Patch<'a> {
    size: usize,
    blake3: [u8; 32],
    edits: &'a [Edit],
}

impl Patch<'_> {
    fn apply(&self, bytes: &mut [u8]) -> bool {
        if bytes.len() != self.size || blake3::hash(bytes).as_bytes() != &self.blake3 {
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
    blake3: [
        0x4d, 0x07, 0xf7, 0x20, 0x1d, 0x61, 0x43, 0xb9, 0xb6, 0xe6, 0x48, 0x98, 0x6c, 0xb9, 0x57,
        0x49, 0x29, 0x42, 0x9b, 0x05, 0xbe, 0x6e, 0xaf, 0xa0, 0xcc, 0x10, 0xcf, 0xc4, 0x90, 0xce,
        0xd6, 0x44,
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
    blake3: [
        0xf0, 0x4c, 0xdf, 0x37, 0x7f, 0x6f, 0x48, 0x45, 0x9a, 0x49, 0x9b, 0x66, 0xb5, 0xea, 0x2f,
        0x4f, 0x4e, 0xab, 0x32, 0x32, 0xbf, 0xbc, 0xfb, 0x23, 0x7d, 0x0c, 0xb8, 0xf0, 0x42, 0x5c,
        0xd1, 0x07,
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
    blake3: [
        0xd4, 0x73, 0xd1, 0xd4, 0xf1, 0x9a, 0x41, 0x90, 0x0e, 0x32, 0xee, 0x08, 0x8c, 0x6e, 0x5c,
        0x5d, 0x0b, 0x11, 0x8e, 0x0a, 0x31, 0x0e, 0x1c, 0x1d, 0x5f, 0x1e, 0x0c, 0x47, 0x4e, 0x03,
        0x69, 0x2c,
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

const BREAKERS_DOS: Patch = Patch {
    size: 232_960,
    blake3: [
        0x79, 0xf1, 0xaf, 0xcc, 0xc9, 0x2c, 0x22, 0x01, 0xb4, 0xfb, 0x50, 0x31, 0x10, 0x65, 0xd0,
        0x18, 0x10, 0x75, 0x75, 0x87, 0xed, 0x6b, 0x8a, 0x79, 0x5a, 0x14, 0x52, 0xce, 0x41, 0x3f,
        0x3e, 0x67,
    ],
    edits: &[
        Edit {
            offset: 0x8308,
            before: &[0x58, 0x00, 0x46, 0x80],
            after: &[0x08, 0x00, 0x10, 0x6f],
        },
        Edit {
            offset: 0x729e,
            before: &[0x58, 0x00, 0x47, 0x80],
            after: &[0x08, 0x00, 0xaa, 0x5e],
        },
        Edit {
            offset: 0x8182,
            before: &[0x2c, 0x00],
            after: &[0x03, 0x00],
        },
    ],
};

pub(super) fn prepare_for_play(bytes: &mut [u8]) {
    MINDWHEEL_DOS.apply(bytes);
    ESSEX_DOS.apply(bytes);
    BRIMSTONE_DOS.apply(bytes);
    BREAKERS_DOS.apply(bytes);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ProgramAddr, decode::decode};

    fn authored_patch(bytes: &[u8]) -> Patch<'static> {
        Patch {
            size: bytes.len(),
            blake3: *blake3::hash(bytes).as_bytes(),
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
