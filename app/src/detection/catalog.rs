// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Curated games, editions, reusable specs and detected file distributions.
//! Engines own runtime choices, palettes, geometry and preview generation.

use super::{Engine, FileRecord, Game, Release};

/// Shared game identity and editorial data, independent of platform or edition.
#[cfg_attr(not(feature = "sdl"), allow(dead_code))]
#[derive(Debug)]
pub struct GameMetadata {
    pub identity: Game,
    pub title: &'static str,
    pub genres: &'static [&'static str],
    /// Short editorial launcher line; must fit in two lines at minimum width.
    pub tagline: &'static str,
}

/// Reusable catalogued presentation features, independent of active options.
#[cfg_attr(not(feature = "sdl"), allow(dead_code))]
#[derive(Debug)]
pub struct SpecsProfile {
    /// Authoring/validation identity; runtime links use direct references.
    #[allow(dead_code)]
    pub id: &'static str,
    pub video_features: &'static [&'static str],
    pub audio_features: &'static [&'static str],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Platform {
    Dos,
    PcBooter,
}

/// Publication/edition facts shared by one or more detected distributions.
#[cfg_attr(not(feature = "sdl"), allow(dead_code))]
#[derive(Debug)]
pub struct Edition {
    /// Internal catalog name; not a claim about an official version designation.
    #[allow(dead_code)]
    pub id: &'static str,
    pub game: &'static GameMetadata,
    /// Application-selected engine loading behavior for this input edition.
    pub engine: Engine,
    /// Stored for cataloguing; launcher placement has not yet been selected.
    #[allow(dead_code)]
    pub platform: Platform,
    /// An edition-specific title; None uses the canonical game title.
    pub title: Option<&'static str>,
    pub year: Option<u16>,
    pub developers: &'static [&'static str],
    pub publishers: &'static [&'static str],
    /// Optional display label, independent of edition and release identities.
    pub label: Option<&'static str>,
    pub specs: &'static SpecsProfile,
}

#[cfg(any(feature = "sdl", test))]
impl Edition {
    pub fn title(&self) -> &'static str {
        self.title.unwrap_or(self.game.title)
    }
    pub fn attribution(&self) -> String {
        let developers = self.developers.join(", ");
        let publishers = self.publishers.join(", ");
        if developers.is_empty() || self.developers == self.publishers {
            publishers
        } else if publishers.is_empty() {
            developers
        } else {
            format!("{developers} / {publishers}")
        }
    }

    /// Short year/developer line for sidebar entries and gallery cards.
    pub fn library_line(&self) -> String {
        let credit = self.developers.join(", ");
        self.year
            .map(|year| year.to_string())
            .into_iter()
            .chain((!credit.is_empty()).then_some(credit))
            .collect::<Vec<_>>()
            .join(" · ")
    }

    pub fn detail_line(&self) -> String {
        self.year
            .map(|year| year.to_string())
            .into_iter()
            .chain(
                [self.attribution(), self.game.genres.join(", ")]
                    .into_iter()
                    .filter(|part| !part.is_empty()),
            )
            .collect::<Vec<_>>()
            .join(" · ")
    }

    pub fn features_line(&self) -> String {
        self.specs
            .video_features
            .iter()
            .chain(self.specs.audio_features)
            .copied()
            .collect::<Vec<_>>()
            .join(" · ")
    }
}

pub static ALLEY_CAT: GameMetadata = GameMetadata {
    identity: Game::AlleyCat,
    title: "Alley Cat",
    genres: &["Arcade action"],
    tagline: "A small cat, a very busy neighbourhood.",
};
pub static MACH3: GameMetadata = GameMetadata {
    identity: Game::Mach3,
    title: "Mach 3",
    genres: &["3D rail shooter"],
    tagline: "Back into the cockpit.",
};

pub static CENTURION: GameMetadata = GameMetadata {
    identity: Game::Centurion,
    title: "Centurion: Defender of Rome",
    genres: &["Strategy", "Action"],
    tagline: "Lead Rome from a legion to an empire.",
};
pub static CENTURION_DOS_SPECS: SpecsProfile = SpecsProfile {
    id: "centurion-dos-vga-adlib",
    video_features: &["320×200", "VGA"],
    audio_features: &["AdLib / PC speaker"],
};
pub static CENTURION_DOS: Edition = Edition {
    id: "centurion-defender-of-rome-dos-100a",
    game: &CENTURION,
    engine: Game::Centurion.engine(),
    platform: Platform::Dos,
    title: None,
    year: Some(1990),
    developers: &["Bits of Magic"],
    publishers: &["Electronic Arts"],
    label: Some("DOS 1.00A"),
    specs: &CENTURION_DOS_SPECS,
};

pub static CENTURION_DOS_100B_SPECS: SpecsProfile = SpecsProfile {
    id: "centurion-dos-vga-ega-adlib",
    video_features: &["320×200", "VGA / EGA"],
    audio_features: &["AdLib / PC speaker"],
};
pub static CENTURION_DOS_100B: Edition = Edition {
    id: "centurion-defender-of-rome-dos-100b",
    engine: Engine::Centurion(centurion::assets::LoadingProfile::new(
        centurion::assets::DosRelease::V100B,
        true,
    )),
    label: Some("DOS 1.00B"),
    specs: &CENTURION_DOS_100B_SPECS,
    ..CENTURION_DOS
};

// Graphics availability describes the verified resource set, independently of
// the DOS program/data layout. Keep the existing editions and release IDs stable.
pub static CENTURION_DOS_EGA: Edition = Edition {
    id: "centurion-defender-of-rome-dos-100a-ega",
    engine: Engine::Centurion(centurion::assets::LoadingProfile::new(
        centurion::assets::DosRelease::V100A,
        true,
    )),
    specs: &CENTURION_DOS_100B_SPECS,
    ..CENTURION_DOS
};
pub static CENTURION_DOS_100B_VGA: Edition = Edition {
    id: "centurion-defender-of-rome-dos-100b-vga",
    engine: Engine::Centurion(centurion::assets::LoadingProfile::new(
        centurion::assets::DosRelease::V100B,
        false,
    )),
    specs: &CENTURION_DOS_SPECS,
    ..CENTURION_DOS_100B
};

pub static MINDWHEEL: GameMetadata = GameMetadata {
    identity: Game::Mindwheel,
    title: "Mindwheel",
    genres: &["Interactive fiction"],
    tagline: "A journey through four minds.",
};
pub static ESSEX: GameMetadata = GameMetadata {
    identity: Game::Essex,
    title: "Essex",
    genres: &["Interactive fiction"],
    tagline: "Board the Starship Essex.",
};
pub static ESSEX_DOS: Edition = Edition {
    id: "essex-dos",
    game: &ESSEX,
    engine: Game::Essex.engine(),
    platform: Platform::Dos,
    title: None,
    year: Some(1985),
    developers: &["Synapse Software"],
    publishers: &["Brøderbund Software"],
    label: None,
    specs: &BTZ_DOS_SPECS,
};
pub static BRIMSTONE: GameMetadata = GameMetadata {
    identity: Game::Brimstone,
    title: "Brimstone: The Dream of Gawain",
    genres: &["Interactive fiction"],
    tagline: "Guide Sir Gawain through the underworld of Ulro.",
};
pub static BRIMSTONE_DOS: Edition = Edition {
    id: "brimstone-dos",
    game: &BRIMSTONE,
    engine: Game::Brimstone.engine(),
    platform: Platform::Dos,
    title: None,
    year: Some(1985),
    developers: &["Synapse Software"],
    publishers: &["Brøderbund Software"],
    label: None,
    specs: &BTZ_DOS_SPECS,
};
pub static BREAKERS: GameMetadata = GameMetadata {
    identity: Game::Breakers,
    title: "Breakers",
    genres: &["Interactive fiction"],
    tagline: "Find your way through an alien world.",
};
pub static BREAKERS_DOS: Edition = Edition {
    id: "breakers-dos",
    game: &BREAKERS,
    engine: Game::Breakers.engine(),
    platform: Platform::Dos,
    title: None,
    year: Some(1986),
    developers: &["Synapse Software"],
    publishers: &["Brøderbund Software"],
    label: None,
    specs: &BTZ_DOS_SPECS,
};
pub static BTZ_DOS_SPECS: SpecsProfile = SpecsProfile {
    id: "btz-dos-text",
    video_features: &["80×25 text"],
    audio_features: &["PC speaker"],
};
pub static MINDWHEEL_DOS: Edition = Edition {
    id: "mindwheel-dos",
    game: &MINDWHEEL,
    engine: Game::Mindwheel.engine(),
    platform: Platform::Dos,
    title: None,
    year: Some(1985),
    developers: &["Synapse Software"],
    publishers: &["Brøderbund Software"],
    label: None,
    specs: &BTZ_DOS_SPECS,
};

pub static ALLEY_CAT_DOS_SPECS: SpecsProfile = SpecsProfile {
    id: "alley-cat-dos",
    video_features: &["320×200", "CGA / PCjr"],
    audio_features: &["PC speaker"],
};
pub static MACH3_DOS_SPECS: SpecsProfile = SpecsProfile {
    id: "mach-3-dos",
    video_features: &["CGA colour / 640×200 monochrome"],
    audio_features: &["PC speaker"],
};

pub static ALLEY_CAT_DOS: Edition = Edition {
    id: "alley-cat-dos",
    game: &ALLEY_CAT,
    engine: Game::AlleyCat.engine(),
    platform: Platform::Dos,
    title: None,
    year: Some(1984),
    developers: &["Synapse Software"],
    publishers: &["IBM"],
    label: None,
    specs: &ALLEY_CAT_DOS_SPECS,
};
/// A later disk container of the same DOS conversion, not an original booter.
pub static ALLEY_CAT_DOS_IMAGE: Edition = Edition {
    id: "alley-cat-dos-image",
    engine: Engine::AlleyCat(alleycat::assets::InputFormat::DosDiskImage),
    label: Some("DOS disk image"),
    ..ALLEY_CAT_DOS
};
/// The NRS enhancement is an input variant, not a newly identified commercial
/// edition. Both supported forms use the same restored game/audio resources.
pub static ALLEY_CAT_RESTORED_SPECS: SpecsProfile = SpecsProfile {
    id: "alley-cat-dos-restored",
    video_features: &["320×200", "CGA / PCjr"],
    audio_features: &["PC speaker / three-voice PSG (PCjr)"],
};
pub static ALLEY_CAT_TANDY: Edition = Edition {
    id: "alley-cat-dos-nrs-tandy",
    engine: Engine::AlleyCat(alleycat::assets::InputFormat::Tandy),
    label: Some("NRS Tandy enhancement"),
    specs: &ALLEY_CAT_RESTORED_SPECS,
    ..ALLEY_CAT_DOS
};
pub static ALLEY_CAT_WITH_PATCH: Edition = Edition {
    id: "alley-cat-dos-nrs-patch",
    engine: Engine::AlleyCat(alleycat::assets::InputFormat::CatWithPatch),
    ..ALLEY_CAT_TANDY
};
pub static ALLEY_CAT_PC_BOOTER_SPECS: SpecsProfile = SpecsProfile {
    id: "alley-cat-pc-booter",
    ..ALLEY_CAT_RESTORED_SPECS
};
/// The verified image is catalogued by its container format. Its modified disk
/// checks do not establish that it is an unmodified commercial release.
pub static ALLEY_CAT_PC_BOOTER: Edition = Edition {
    id: "alley-cat-pc-booter",
    engine: Engine::AlleyCat(alleycat::assets::InputFormat::PcBooter),
    platform: Platform::PcBooter,
    label: Some("PC Booter"),
    specs: &ALLEY_CAT_PC_BOOTER_SPECS,
    ..ALLEY_CAT_DOS
};

pub static MACH3_DOS: Edition = Edition {
    id: "mach-3-dos",
    game: &MACH3,
    engine: Game::Mach3.engine(),
    platform: Platform::Dos,
    title: None,
    year: Some(1987),
    developers: &["Loriciels"],
    publishers: &["Loriciels"],
    label: None,
    specs: &MACH3_DOS_SPECS,
};

// Both executable packagings expand to the same program and use these same
// external resources. Keep each distribution's complete verification manifest.
const MACH3_DATA_FILES: [FileRecord; 17] = [
    FileRecord {
        name: "DATA1",
        size: 16_384,

        blake3: "aec159c7e7942e4ad57047d92aa2ddbaea64c898f76dd7265e1abff16d88f3ba",
    },
    FileRecord {
        name: "DATA2",
        size: 28_960,

        blake3: "7a0a8d67855bcda56384bd4ea18c92a35d302ea2a8a2c9bf5691b8353fe9ab44",
    },
    FileRecord {
        name: "DATA3",
        size: 16_000,

        blake3: "3acee17ded55afccbea75d458ad8ec1fcc2d8d03244052db8caedd51be01576b",
    },
    FileRecord {
        name: "DATA4",
        size: 25_704,

        blake3: "38b303f55bf076cc2118483f782cce4516378586f168211d0c54b2559f1fb260",
    },
    FileRecord {
        name: "DATA5",
        size: 25_704,

        blake3: "668bad553bb947b8d5be1e0e3653574197e606911d898199ebc04e43c06df8e1",
    },
    FileRecord {
        name: "DATA6",
        size: 16_384,

        blake3: "0fedddf4a90f96b4abbf049618d13c24cfb323f92d060e73e4293777b3e2c72b",
    },
    FileRecord {
        name: "DATA7",
        size: 11_872,

        blake3: "cf2e9a8fc84770206b7e269edbf47a7679442d0e4388a2614a2c428752afcb6e",
    },
    FileRecord {
        name: "DATA8",
        size: 9_723,

        blake3: "b601c1c491a1433d9ac86e6c76c6285223c7d383af84989e88d29225c85c3adf",
    },
    FileRecord {
        name: "DATA9",
        size: 10_874,

        blake3: "b5c94cbe9cec91f03cce4f93ee1ab94437c77e03d66c4950667ca5f06b5a1132",
    },
    FileRecord {
        name: "DATAA",
        size: 19_690,

        blake3: "82e54ad4fda7019226d9d9c6d117773d594207c3354c4a938618477aedfb8d45",
    },
    FileRecord {
        name: "DATAB",
        size: 15_500,

        blake3: "7b4dfac8d1f5d76fa47f4b885cbb6658282e859b9e45732a093818fa7c59e771",
    },
    FileRecord {
        name: "DATAC",
        size: 8_700,

        blake3: "cebec6155b621780bb5e187ef1a6f901ae8958c3535f431067bc082fdd6ba0c4",
    },
    FileRecord {
        name: "DATAD",
        size: 34_458,

        blake3: "170af134372197d306ee2bf03cd07a67a3744ce54c21d291a5d488a5ae591309",
    },
    FileRecord {
        name: "DATAE",
        size: 8_700,

        blake3: "b3165eaa7b890afc277838e88b27b12835ec573424d4d417e310cce8e4fa54f0",
    },
    FileRecord {
        name: "DATAF",
        size: 8_664,

        blake3: "d8274e09437bcab97400c6736abf472c87af22d01be57f7fd114db9e3dbbfbbb",
    },
    FileRecord {
        name: "DATAG",
        size: 8_802,

        blake3: "989dce0c45fbf05b00d5c71ebcc3e03d0a17a43efa05df906759cc7eb23f0b09",
    },
    FileRecord {
        name: "DATAH",
        size: 8_927,

        blake3: "21ea06799ffafa57e890c51b7cb8724b86f92d8f51f19edb850d3e38dbf3abcf",
    },
];

const fn mach3_files(executable: FileRecord) -> [FileRecord; 18] {
    let mut files = [executable; 18];
    let mut index = 0;
    while index < MACH3_DATA_FILES.len() {
        files[index + 1] = MACH3_DATA_FILES[index];
        index += 1;
    }
    files
}

// These fingerprints identify the verified inputs used by the engines. Their
// historical commercial release/version identities remain unknown. Only files
// needed by the current implementations are required; unrelated extras are fine.
// The executable is not loaded: only the overlay data and resource banks are
// required. Do not bind supported play to an unused DOS launcher/protection copy.
const CENTURION_DOS_FILES: [FileRecord; 19] = [
    FileRecord {
        name: "BATL.DAT",
        size: 96_479,
        blake3: "54e080ab17f5292cfbac0aeb64b4cfcc8c527ce2e4279b22cc7f5b814cc10acf",
    },
    FileRecord {
        name: "BATL.DIR",
        size: 440,
        blake3: "7923d581a24f6e16dcc344ec900329086abd383e53abe56a7bf20f723b16becb",
    },
    FileRecord {
        name: "CENT.DAT",
        size: 14_770,
        blake3: "dfe7772e91053ac99292af9c56b7d27ff12843a428417f583c4d66b38b287701",
    },
    FileRecord {
        name: "CENT.DIR",
        size: 240,
        blake3: "3738ef7cfd586e92793fabec8cc60b393713e43b1c35be105968225dd53886f8",
    },
    FileRecord {
        name: "MGLAD.DAT",
        size: 72_758,
        blake3: "b1d894f65927b1b8e30cba5e4ca6aac0799e4ab55723a4d27926f6a0529557f9",
    },
    FileRecord {
        name: "MGLAD.DIR",
        size: 480,
        blake3: "9e55a9850fb4875330b0ce85b56184d36234e72f14d4165e44ebd1a9f7a58384",
    },
    FileRecord {
        name: "MINIT.DAT",
        size: 41_349,
        blake3: "5600d4527ea2f9171d85a5f67f3dc8fed38c2234088c4475805b48c36ca49fcf",
    },
    FileRecord {
        name: "MINIT.DIR",
        size: 140,
        blake3: "157fb02ddb80ef98e8bf2c131fa8c8e64b02a78a4d6df2eb848f4b1b16609cc7",
    },
    FileRecord {
        name: "MMAP.DAT",
        size: 23_516,
        blake3: "f5e53a1858b6503da2526a06900cf60d393432614631aa36134eb7b3672377a9",
    },
    FileRecord {
        name: "MMAP.DIR",
        size: 40,
        blake3: "36df7e803168bd2e021e2586381c9444d20292089b2ad47a310e7fe8cb775c60",
    },
    FileRecord {
        name: "MNAVAL.DAT",
        size: 34_713,
        blake3: "896915633132339284214caedeaaae09ec412afdd8a22e5c362988fd13c6bc94",
    },
    FileRecord {
        name: "MNAVAL.DIR",
        size: 500,
        blake3: "581a17296db1638ea85099644f1c093c3fdeeb0e309b7bc997c043fb8e238fce",
    },
    FileRecord {
        name: "MNEGO.DAT",
        size: 196_775,
        blake3: "dd83599faa790222cf3babbc162b03b70f031127bb6d1734d2bc51aee39ebb17",
    },
    FileRecord {
        name: "MNEGO.DIR",
        size: 2_040,
        blake3: "fd223c053f85978af305b3c09e825cad8dbac6ec9306bd3359d961de9d805d8e",
    },
    FileRecord {
        name: "MRACE.DAT",
        size: 49_309,
        blake3: "62cb9280d5c7dceb526f506567682264419f7dc74aa979f83f48926b3a63a321",
    },
    FileRecord {
        name: "MRACE.DIR",
        size: 280,
        blake3: "975390bcd0ad2c0427a9d12c10c5b7ec105d9f613e482e3170898e48e47f67c6",
    },
    FileRecord {
        name: "NAVL.DAT",
        size: 34_705,
        blake3: "64c00916a0a92852a4d8afc15c4021f7ee6b7648de8786532ab1208f64159067",
    },
    FileRecord {
        name: "NAVL.DIR",
        size: 100,
        blake3: "9ab394013adfba5be1f3b99896d188dca13511f618500e9f81759a117af8f02b",
    },
    FileRecord {
        name: "OVL0.OVL",
        size: 96_272,
        blake3: "a785df2681d83c6f6448c1ca48d2f38e75b67ace5c5b0c9539814a02e8a25377",
    },
];

// Complete resource set for the examined 1.00B VGA/EGA distribution.
const CENTURION_DOS_100B_FILES: [FileRecord; 31] = [
    FileRecord {
        name: "BATL.DAT",
        size: 96_479,
        blake3: "54e080ab17f5292cfbac0aeb64b4cfcc8c527ce2e4279b22cc7f5b814cc10acf",
    },
    FileRecord {
        name: "BATL.DIR",
        size: 440,
        blake3: "7923d581a24f6e16dcc344ec900329086abd383e53abe56a7bf20f723b16becb",
    },
    FileRecord {
        name: "CENT.DAT",
        size: 14_770,
        blake3: "ad8db5c5ebf3b4c6f324d6267b7b4e080decd25d2e797374d426f91d317adb20",
    },
    FileRecord {
        name: "CENT.DIR",
        size: 240,
        blake3: "3738ef7cfd586e92793fabec8cc60b393713e43b1c35be105968225dd53886f8",
    },
    FileRecord {
        name: "EGLAD.DAT",
        size: 52_319,
        blake3: "6cfb35ace345a8fc6e1ef161fe148bfaa6da839dbb633b6ca27a010bc37daef8",
    },
    FileRecord {
        name: "EGLAD.DIR",
        size: 480,
        blake3: "8e54d333497dc86f8ae851ffe4cea3f07cc5d7e3e84d2799f93cb6bc8574e580",
    },
    FileRecord {
        name: "EINIT.DAT",
        size: 25_508,
        blake3: "5cf5a0185b3b49e6aeb5e438375f8bb0837d5ef916da2ec82ed62a266cbd2299",
    },
    FileRecord {
        name: "EINIT.DIR",
        size: 140,
        blake3: "01ab1691c1988856b9de9818663a03022b41d375f1de6038ba677576eca977b0",
    },
    FileRecord {
        name: "EMAP.DAT",
        size: 20_221,
        blake3: "70066aefbdccf7095e5504586745737fb9f0d57af3ac868c131c29664731d295",
    },
    FileRecord {
        name: "EMAP.DIR",
        size: 40,
        blake3: "d9edfa31feab12a7c02486f0c0e96d676e46894d1fda1f54c858adf788e0eedf",
    },
    FileRecord {
        name: "ENAVAL.DAT",
        size: 38_654,
        blake3: "4a97b17b2035ed933357afd11a1b5b711070f0189e002bcae14a633902b83d9b",
    },
    FileRecord {
        name: "ENAVAL.DIR",
        size: 520,
        blake3: "7c7e8870e61bf1f9d2efacaa5355523e493781e390da5ec21f111e864cb38e70",
    },
    FileRecord {
        name: "ENEGO.DAT",
        size: 161_108,
        blake3: "23806688ae404a50ce81450d2e81bdae9e5ab98eaa8af3b3e279bda3d2170991",
    },
    FileRecord {
        name: "ENEGO.DIR",
        size: 2_040,
        blake3: "3c8c46b05c0c724287ddd32b2908553ccc678c191f5723d3ba8e62874369ac7d",
    },
    FileRecord {
        name: "ERACE.DAT",
        size: 39_629,
        blake3: "ae1ff8f5386eae29a10bcde3d9ae88775ddc95cdfa4e1bb5101d8fa5971ef370",
    },
    FileRecord {
        name: "ERACE.DIR",
        size: 280,
        blake3: "7a2fa352fc5a8420c50ace1eb9a00c1ca1e8587da0f8cfeb03d2b6496d24e762",
    },
    FileRecord {
        name: "MGLAD.DAT",
        size: 72_758,
        blake3: "b1d894f65927b1b8e30cba5e4ca6aac0799e4ab55723a4d27926f6a0529557f9",
    },
    FileRecord {
        name: "MGLAD.DIR",
        size: 480,
        blake3: "9e55a9850fb4875330b0ce85b56184d36234e72f14d4165e44ebd1a9f7a58384",
    },
    FileRecord {
        name: "MINIT.DAT",
        size: 41_349,
        blake3: "5600d4527ea2f9171d85a5f67f3dc8fed38c2234088c4475805b48c36ca49fcf",
    },
    FileRecord {
        name: "MINIT.DIR",
        size: 140,
        blake3: "157fb02ddb80ef98e8bf2c131fa8c8e64b02a78a4d6df2eb848f4b1b16609cc7",
    },
    FileRecord {
        name: "MMAP.DAT",
        size: 23_516,
        blake3: "f5e53a1858b6503da2526a06900cf60d393432614631aa36134eb7b3672377a9",
    },
    FileRecord {
        name: "MMAP.DIR",
        size: 40,
        blake3: "36df7e803168bd2e021e2586381c9444d20292089b2ad47a310e7fe8cb775c60",
    },
    FileRecord {
        name: "MNAVAL.DAT",
        size: 34_713,
        blake3: "896915633132339284214caedeaaae09ec412afdd8a22e5c362988fd13c6bc94",
    },
    FileRecord {
        name: "MNAVAL.DIR",
        size: 500,
        blake3: "581a17296db1638ea85099644f1c093c3fdeeb0e309b7bc997c043fb8e238fce",
    },
    FileRecord {
        name: "MNEGO.DAT",
        size: 196_775,
        blake3: "dd83599faa790222cf3babbc162b03b70f031127bb6d1734d2bc51aee39ebb17",
    },
    FileRecord {
        name: "MNEGO.DIR",
        size: 2_040,
        blake3: "fd223c053f85978af305b3c09e825cad8dbac6ec9306bd3359d961de9d805d8e",
    },
    FileRecord {
        name: "MRACE.DAT",
        size: 49_309,
        blake3: "62cb9280d5c7dceb526f506567682264419f7dc74aa979f83f48926b3a63a321",
    },
    FileRecord {
        name: "MRACE.DIR",
        size: 280,
        blake3: "975390bcd0ad2c0427a9d12c10c5b7ec105d9f613e482e3170898e48e47f67c6",
    },
    FileRecord {
        name: "NAVL.DAT",
        size: 34_705,
        blake3: "64c00916a0a92852a4d8afc15c4021f7ee6b7648de8786532ab1208f64159067",
    },
    FileRecord {
        name: "NAVL.DIR",
        size: 100,
        blake3: "9ab394013adfba5be1f3b99896d188dca13511f618500e9f81759a117af8f02b",
    },
    FileRecord {
        name: "OVL0.OVL",
        size: 96_272,
        blake3: "4c5fc346e56a06d6000240d6166f5bcc504e3abff3e75d1da8f12d112c64b00f",
    },
];

// All shared banks match byte for byte. The examined A package lacks its
// EGA-only banks: recognize the B EGA banks with A's own campaign and overlay.
// This is an assembled supported combination, not a separately verified A package.
const CENTURION_DOS_EGA_FILES: [FileRecord; 31] = {
    let mut files = CENTURION_DOS_100B_FILES;
    files[2] = CENTURION_DOS_FILES[2]; // CENT.DAT: A campaign template
    files[30] = CENTURION_DOS_FILES[18]; // OVL0.OVL: A layout
    files
};
const CENTURION_DOS_100B_VGA_FILES: [FileRecord; 19] = {
    let mut files = CENTURION_DOS_FILES;
    files[2] = CENTURION_DOS_100B_FILES[2];
    files[18] = CENTURION_DOS_100B_FILES[30];
    files
};

pub static RELEASES: &[Release] = &[
    Release {
        id: "alley-cat-dos-7b92bb401f",
        compatibility_id: "alley-cat-dos",
        edition: &ALLEY_CAT_DOS,
        #[cfg(test)]
        identity_files: &[],
        files: &[FileRecord {
            name: "CAT.EXE",
            size: 55_067,

            blake3: "8b6c0f5151075e596296675648eb75b4527e228f74aff1b5db2403f03ff84cf2",
        }],
    },
    Release {
        id: "mach-3-dos-3d1f0c95f9",
        compatibility_id: "mach-3-dos",
        edition: &MACH3_DOS,
        #[cfg(test)]
        identity_files: &[],
        files: &mach3_files(FileRecord {
            name: "MACH3.EXE",
            size: 43_008,
            blake3: "4d91473bd4444c22a44f51fc2a2e5d13206d06ad74a5f8c31438e7edf35efb4c",
        }),
    },
    Release {
        id: "mindwheel-dos-057c9533e4",
        compatibility_id: "mindwheel-dos",
        edition: &MINDWHEEL_DOS,
        #[cfg(test)]
        identity_files: &[],
        files: &[
            FileRecord {
                name: "ADISK.OBJ",
                size: 188_928,

                blake3: "4d07f7201d6143b9b6e648986cb9574929429b05be6eafa0cc10cfc490ced644",
            },
            FileRecord {
                name: "GO.EXE",
                size: 33_280,

                blake3: "abc0678c92dc91f39a0203c19e80afed9564c32250db21216c904427b5cdd756",
            },
            FileRecord {
                name: "SIDE.TXT",
                size: 65,

                blake3: "a138ce6054a71b74b1f8dfd56908cfb463ab45c249f19b4379e48f86c0a907c3",
            },
        ],
    },
    Release {
        id: "essex-dos-226e565be2",
        compatibility_id: "essex-dos",
        edition: &ESSEX_DOS,
        #[cfg(test)]
        identity_files: &[],
        files: &[
            FileRecord {
                name: "ADISK.OBJ",
                size: 180736,

                blake3: "f04cdf377f6f48459a499b66b5ea2f4f4eab3232bfbcfb237d0cb8f0425cd107",
            },
            FileRecord {
                name: "GO.EXE",
                size: 35328,

                blake3: "1d3d082d6ebc8f7c5a1c49ca7c776653d0b3755068ac2fb972f02b6b13feea98",
            },
            FileRecord {
                name: "SIDE.TXT",
                size: 65,

                blake3: "248731936ee31169d6cab8f54e8215bb56b14d893912a13d2d30497cd75862ce",
            },
        ],
    },
    Release {
        id: "brimstone-dos-d3bec28797",
        compatibility_id: "brimstone-dos",
        edition: &BRIMSTONE_DOS,
        #[cfg(test)]
        identity_files: &[],
        files: &[
            FileRecord {
                name: "ADISK.OBJ",
                size: 183_296,

                blake3: "d473d1d4f19a41900e32ee088c6e5c5d0b118e0a310e1c1d5f1e0c474e03692c",
            },
            FileRecord {
                name: "GO.EXE",
                size: 35_328,

                blake3: "758b53ef59d01a191270b6971fea44b1e0e1dfcd4720674afd6e376d1d0ae8e5",
            },
            FileRecord {
                name: "SIDE.TXT",
                size: 65,

                blake3: "248731936ee31169d6cab8f54e8215bb56b14d893912a13d2d30497cd75862ce",
            },
        ],
    },
    Release {
        id: "breakers-dos-d5569f4b23",
        compatibility_id: "breakers-dos",
        edition: &BREAKERS_DOS,
        #[cfg(test)]
        identity_files: &[],
        files: &[
            FileRecord {
                name: "ADISK.OBJ",
                size: 232960,

                blake3: "79f1afccc92c2201b4fb50311065d01810757587ed6b8a795a1452ce413f3e67",
            },
            FileRecord {
                name: "RUN.EXE",
                size: 35328,

                blake3: "95aa4027bef41da21a3fa636be571ab503a0fc3da1353150f750714cd5acb6ac",
            },
            FileRecord {
                name: "SIDE.TXT",
                size: 65,

                blake3: "248731936ee31169d6cab8f54e8215bb56b14d893912a13d2d30497cd75862ce",
            },
            FileRecord {
                name: "TITLE.EXE",
                size: 3584,

                blake3: "d49cde59b5c1502dfbd939c90676b4176329b5101bb99821dd4f284704176278",
            },
        ],
    },
    Release {
        id: "mach-3-dos-564352e19f",
        compatibility_id: "mach-3-dos",
        edition: &MACH3_DOS,
        #[cfg(test)]
        identity_files: &[],
        files: &mach3_files(FileRecord {
            name: "MACH3.EXE",
            size: 23_929,
            blake3: "bb0a6af8c371c234385875c23e7c4936965119f2778b8d4ed8940c28543f95ae",
        }),
    },
    Release {
        id: "mach-3-dos-549ea638df",
        compatibility_id: "mach-3-dos",
        edition: &MACH3_DOS,
        #[cfg(test)]
        identity_files: &[],
        files: &mach3_files(FileRecord {
            name: "MACH3.OV2",
            size: 43_007,
            blake3: "ec269441b8183e81913c253d098ec18686cc844604144e5c3071efd3d0d277cb",
        }),
    },
    Release {
        id: "alley-cat-dos-17b9603eee",
        compatibility_id: "alley-cat-dos",
        edition: &ALLEY_CAT_TANDY,
        #[cfg(test)]
        identity_files: &[],
        files: &[FileRecord {
            name: "TANDYCAT.EXE",
            size: 56_881,
            blake3: "1e162b41fd7516ed6063c33a9431ef2df268ca0720e25d2c34918ba084063bc9",
        }],
    },
    Release {
        id: "alley-cat-dos-bc3e9d804a",
        compatibility_id: "alley-cat-dos",
        edition: &ALLEY_CAT_WITH_PATCH,
        #[cfg(test)]
        identity_files: &[],
        files: &[
            FileRecord {
                name: "CAT.EXE",
                size: 55_067,
                blake3: "8b6c0f5151075e596296675648eb75b4527e228f74aff1b5db2403f03ff84cf2",
            },
            FileRecord {
                name: "791CE265.DIF",
                size: 6_704,
                blake3: "8b4a0b641691d33b146f00806bf5fc77a91a769b684388f7ce7ed0826922758b",
            },
        ],
    },
    Release {
        id: "alley-cat-pc-booter-56647cbe98",
        compatibility_id: "alley-cat-dos",
        edition: &ALLEY_CAT_PC_BOOTER,
        #[cfg(test)]
        identity_files: &[],
        files: &[FileRecord {
            name: "disk1.img",
            size: 184_320,
            blake3: "f6f0fe51a58b2c32995427fd8f3af2b1df27f3f4b8033a4a2b55946e3a90c900",
        }],
    },
    Release {
        id: "alley-cat-dos-d7d4bbbcbd",
        compatibility_id: "alley-cat-dos",
        edition: &ALLEY_CAT_DOS_IMAGE,
        #[cfg(test)]
        identity_files: &[],
        files: &[FileRecord {
            name: "alley_cat.img",
            size: 3_145_728,
            blake3: "2b8cf7b87eb8975f9ad502428d39c8fbaa57cb2aacd16f0f2e79d4b3f2aee4b8",
        }],
    },
    Release {
        id: "centurion-defender-of-rome-dos-b82e91fe62",
        compatibility_id: "centurion-defender-of-rome-dos-100a",
        edition: &CENTURION_DOS,
        #[cfg(test)]
        identity_files: &[],
        files: &CENTURION_DOS_FILES,
    },
    Release {
        id: "centurion-defender-of-rome-dos-4d5fd76b5e",
        // The releases retain the same DOS save schema and option meanings.
        // Keep the existing key stable; per-install video availability differs.
        compatibility_id: "centurion-defender-of-rome-dos-100a",
        edition: &CENTURION_DOS_100B,
        #[cfg(test)]
        identity_files: &[],
        files: &CENTURION_DOS_100B_FILES,
    },
    Release {
        id: "centurion-defender-of-rome-dos-65d635b70a",
        compatibility_id: "centurion-defender-of-rome-dos-100a",
        edition: &CENTURION_DOS_EGA,
        #[cfg(test)]
        identity_files: &[],
        files: &CENTURION_DOS_EGA_FILES,
    },
    Release {
        id: "centurion-defender-of-rome-dos-95afd5852a",
        compatibility_id: "centurion-defender-of-rome-dos-100a",
        edition: &CENTURION_DOS_100B_VGA,
        #[cfg(test)]
        identity_files: &[],
        files: &CENTURION_DOS_100B_VGA_FILES,
    },
];

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn alleycat_restoration_forms_select_loading_profiles_and_share_compatibility() {
        use alleycat::assets::InputFormat;
        let releases = RELEASES
            .iter()
            .filter(|release| release.game() == Game::AlleyCat)
            .collect::<Vec<_>>();
        assert_eq!(releases.len(), 5);
        assert_eq!(releases[0].engine(), Engine::AlleyCat(InputFormat::Cat));
        assert_eq!(releases[1].engine(), Engine::AlleyCat(InputFormat::Tandy));
        assert_eq!(
            releases[2].engine(),
            Engine::AlleyCat(InputFormat::CatWithPatch)
        );
        assert_eq!(
            releases[3].engine(),
            Engine::AlleyCat(InputFormat::PcBooter)
        );
        assert_eq!(
            releases[4].engine(),
            Engine::AlleyCat(InputFormat::DosDiskImage)
        );
        assert_eq!(releases[4].edition.platform, Platform::Dos);
        assert_eq!(releases[4].edition.label, Some("DOS disk image"));
        assert!(std::ptr::eq(
            releases[4].edition.specs,
            releases[0].edition.specs
        ));
        assert_eq!(releases[1].files[0].name, "TANDYCAT.EXE");
        assert_eq!(releases[2].files[0].name, "CAT.EXE");
        assert_eq!(releases[2].files[1].name, "791CE265.DIF");
        for release in &releases[1..] {
            assert!(releases[0].compatible_with(release));
            assert!(release.compatible_with(releases[0]));
            if release.engine() != Engine::AlleyCat(InputFormat::DosDiskImage) {
                assert!(!std::ptr::eq(
                    release.edition.specs,
                    releases[0].edition.specs
                ));
            }
        }
        assert!(std::ptr::eq(
            releases[1].edition.specs,
            releases[2].edition.specs
        ));
        assert_eq!(releases[0].id, "alley-cat-dos-7b92bb401f");
        assert_eq!(releases[1].id, "alley-cat-dos-17b9603eee");
        assert_eq!(releases[2].id, "alley-cat-dos-bc3e9d804a");
    }

    #[test]
    fn pc_booter_has_a_distinct_platform_edition_and_native_specs() {
        let release = RELEASES
            .iter()
            .find(|release| release.edition.platform == Platform::PcBooter)
            .unwrap();
        assert_eq!(release.id, "alley-cat-pc-booter-56647cbe98");
        assert_eq!(release.compatibility_id, "alley-cat-dos");
        assert!(release.compatible_with(&RELEASES[0]));
        assert_eq!(release.files.len(), 1);
        assert_eq!(release.files[0].name, "disk1.img");
        assert_eq!(release.files[0].size, 184_320);
        assert_eq!(
            release.files[0].blake3,
            "f6f0fe51a58b2c32995427fd8f3af2b1df27f3f4b8033a4a2b55946e3a90c900"
        );
        let edition = release.edition;
        assert!(std::ptr::eq(edition.game, &ALLEY_CAT));
        assert_ne!(edition.id, ALLEY_CAT_DOS.id);
        assert_ne!(edition.id, ALLEY_CAT_TANDY.id);
        assert_ne!(edition.id, ALLEY_CAT_WITH_PATCH.id);
        assert_eq!(edition.label, Some("PC Booter"));
        assert_eq!(edition.year, Some(1984));
        assert_eq!(edition.developers, &["Synapse Software"]);
        assert_eq!(edition.publishers, &["IBM"]);
        assert!(!std::ptr::eq(edition.specs, ALLEY_CAT_TANDY.specs));
        assert_eq!(edition.specs.video_features, &["320×200", "CGA / PCjr"]);
        assert_eq!(
            edition.specs.audio_features,
            &["PC speaker / three-voice PSG (PCjr)"]
        );
        assert_eq!(
            edition.features_line(),
            "320×200 · CGA / PCjr · PC speaker / three-voice PSG (PCjr)"
        );
    }

    #[test]
    fn mach3_packagings_share_resources_edition_and_compatibility() {
        let versions: Vec<_> = RELEASES
            .iter()
            .filter(|r| r.game() == super::super::Game::Mach3)
            .collect();
        assert_eq!(versions.len(), 3);
        let (reference, packed) = (versions[0], versions[1]);
        assert_ne!(reference.id, packed.id);
        assert!(reference.compatible_with(packed));
        assert!(packed.compatible_with(reference));
        assert_eq!(packed.compatibility_id, "mach-3-dos");
        assert!(std::ptr::eq(reference.edition, packed.edition));
        assert_eq!(reference.files.len(), 18);
        assert_eq!(packed.files.len(), 18);
        assert_ne!(reference.files[0].blake3, packed.files[0].blake3);
        assert_eq!(packed.files[0].size, 23_929);
        let overlay = versions[2];
        assert_eq!(overlay.files[0].name, "MACH3.OV2");
        assert_eq!(overlay.files[0].size, 43_007);
        assert!(
            !overlay
                .files
                .iter()
                .any(|file| matches!(file.name, "MACH3.EXE" | "MACH3.OV1"))
        );
        for distribution in &versions[1..] {
            assert_ne!(reference.id, distribution.id);
            assert!(reference.compatible_with(distribution));
            assert!(distribution.compatible_with(reference));
            assert!(std::ptr::eq(reference.edition, distribution.edition));
            assert_eq!(distribution.files.len(), 18);
            for (left, right) in reference.files[1..].iter().zip(&distribution.files[1..]) {
                assert_eq!(
                    (left.name, left.size, left.blake3),
                    (right.name, right.size, right.blake3)
                );
            }
        }
    }

    #[test]
    fn edition_lines_handle_multiple_credits_and_absent_facts() {
        static SHARED_GAME: GameMetadata = GameMetadata {
            genres: &["Action", "Adventure"],
            ..ALLEY_CAT
        };
        static EMPTY_SPECS: SpecsProfile = SpecsProfile {
            id: "synthetic-empty-specs",
            video_features: &[],
            audio_features: &[],
        };
        let mut edition = Edition {
            game: &SHARED_GAME,
            developers: &["Studio A", "Studio B"],
            publishers: &["Publisher A", "Publisher B"],
            label: Some("Reissue"),
            ..ALLEY_CAT_DOS
        };
        assert_eq!(edition.library_line(), "1984 · Studio A, Studio B");
        assert_eq!(
            edition.detail_line(),
            "1984 · Studio A, Studio B / Publisher A, Publisher B · Action, Adventure"
        );
        edition.publishers = edition.developers;
        assert_eq!(edition.attribution(), "Studio A, Studio B");
        edition.developers = &[];
        assert_eq!(edition.attribution(), "Studio A, Studio B");
        edition.year = None;
        edition.publishers = &[];
        edition.specs = &EMPTY_SPECS;
        assert!(edition.library_line().is_empty());
        assert!(edition.attribution().is_empty());
        assert_eq!(edition.detail_line(), "Action, Adventure");
        assert!(edition.features_line().is_empty());
        edition.developers = &["Studio A"];
        assert_eq!(edition.attribution(), "Studio A");
    }

    #[test]
    fn distributions_share_editions_and_editions_share_games_and_specs() {
        static REPRINT: Edition = Edition {
            id: "synthetic-reprint",
            title: Some("Localized test title"),
            year: Some(1985),
            publishers: &["Test publisher"],
            ..ALLEY_CAT_DOS
        };
        let original = &RELEASES[0];
        let mut another = Release {
            id: "synthetic-other-executable",
            #[cfg(test)]
            identity_files: &[],
            files: &[],
            ..*original
        };
        assert!(std::ptr::eq(another.edition, original.edition));
        // Same edition does not imply compatibility.
        another.compatibility_id = "synthetic-incompatible-saves";
        assert!(!another.compatible_with(original));
        another.edition = &REPRINT;
        assert!(std::ptr::eq(another.edition.game, original.edition.game));
        assert!(std::ptr::eq(another.edition.specs, original.edition.specs));
        assert_eq!(another.game(), original.game());
        assert_eq!(another.edition.title(), "Localized test title");
        assert_eq!(original.edition.title(), "Alley Cat");
        assert_eq!(another.edition.year, Some(1985));
        assert_eq!(original.edition.year, Some(1984));
        assert_ne!(another.edition.publishers, original.edition.publishers);
        // Different editions can explicitly share a compatibility group.
        another.compatibility_id = original.compatibility_id;
        assert!(another.compatible_with(original));
    }

    #[test]
    fn spec_changes_are_explicit_and_independent_of_shared_game_data() {
        static OTHER_SPECS: SpecsProfile = SpecsProfile {
            id: "synthetic-other-video",
            video_features: &["Synthetic video mode"],
            audio_features: ALLEY_CAT_DOS_SPECS.audio_features,
        };
        let edition = Edition {
            specs: &OTHER_SPECS,
            ..ALLEY_CAT_DOS
        };
        assert!(std::ptr::eq(edition.game, ALLEY_CAT_DOS.game));
        assert!(!std::ptr::eq(edition.specs, ALLEY_CAT_DOS.specs));
        assert_eq!(edition.features_line(), "Synthetic video mode · PC speaker");
        assert_eq!(
            ALLEY_CAT_DOS.features_line(),
            "320×200 · CGA / PCjr · PC speaker"
        );
    }

    #[test]
    fn catalog_links_have_unique_ids_and_structured_values() {
        fn register<T>(
            records: &mut BTreeMap<&'static str, &'static T>,
            id: &'static str,
            record: &'static T,
        ) {
            assert!(!id.is_empty());
            assert!(
                id.bytes()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
            );
            if let Some(previous) = records.insert(id, record) {
                assert!(
                    std::ptr::eq(previous, record),
                    "different records reuse ID {id}"
                );
            }
        }
        let (mut games, mut editions, mut profiles) =
            (BTreeMap::new(), BTreeMap::new(), BTreeMap::new());
        for release in RELEASES {
            let edition = release.edition;
            let game = edition.game;
            let specs = edition.specs;
            register(&mut games, game.identity.id(), game);
            register(&mut editions, edition.id, edition);
            register(&mut profiles, specs.id, specs);
            assert!(std::ptr::eq(game.identity.metadata(), game));
            assert!(!game.title.trim().is_empty());
            assert!(!edition.title().trim().is_empty());
            assert!(edition.year.is_none_or(|year| year > 0));
            for values in [
                edition.developers,
                edition.publishers,
                game.genres,
                specs.video_features,
                specs.audio_features,
            ] {
                for value in values {
                    assert!(!value.is_empty());
                    assert_eq!(*value, value.trim());
                    assert!(!value.contains(" · "), "store separate list items");
                }
            }
        }
        assert_eq!(ALLEY_CAT_DOS.platform, Platform::Dos);
        assert_eq!(MACH3_DOS.platform, Platform::Dos);
    }
}
