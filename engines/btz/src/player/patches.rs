// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Release-specific playback edits, separate from the wordcode interpreter.

use super::DosRelease;

struct Edit {
    offset: usize,
    before: &'static [u8],
    after: &'static [u8],
}

struct Patch<'a> {
    size: usize,
    edits: &'a [Edit],
}

impl Patch<'_> {
    fn apply(&self, bytes: &mut [u8]) -> bool {
        // Normal loading already has the application's verified release.
        // Keep local size/instruction guards without identifying the file again.
        if bytes.len() != self.size {
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

pub(super) fn prepare_for_play(release: DosRelease, bytes: &mut [u8]) {
    let patch = match release {
        DosRelease::Mindwheel => &MINDWHEEL_DOS,
        DosRelease::Essex => &ESSEX_DOS,
        DosRelease::Brimstone => &BRIMSTONE_DOS,
        DosRelease::Breakers => &BREAKERS_DOS,
    };
    patch.apply(bytes);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ProgramAddr, decode::decode};

    fn authored_patch(bytes: &[u8]) -> Patch<'static> {
        Patch {
            size: bytes.len(),
            edits: &[Edit {
                offset: 2,
                before: &[0x2c, 0, 0, 0],
                after: &[0x08, 0, 0x20, 0],
            }],
        }
    }

    #[test]
    fn selected_patch_changes_only_its_guarded_instruction_and_is_idempotent() {
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
    fn selected_patch_checks_instructions_without_reidentifying_unrelated_bytes() {
        let original = [0xaa, 0xbb, 0x2c, 0, 0, 0, 0xcc, 0xdd];
        let patch = authored_patch(&original);
        let mut bytes = original;
        bytes[7] ^= 1;
        assert!(patch.apply(&mut bytes));
        assert_eq!(bytes, [0xaa, 0xbb, 0x08, 0, 0x20, 0, 0xcc, 0xdc]);
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
