// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
use std::path::{Path, PathBuf};

use crate::assets::{Assets, Bitmap, Result, read, word};
use crate::graphics::Frame;
use crate::rng::OriginalRng;
use crate::state::Campaign;
mod soak;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Intro,
    Title,
    Credits,
    Prologue,
    Map,
    RegionInspect,
    Rome,
    Legions,
    RaiseLegion,
    LegionList,
    LegionSelect,
    LegionMessage,
    LegionMove,
    Diplomacy,
    BattlePreparation,
    Encounter,
    Naval,
    Games,
    TurnRevolt,
    TurnEnemy,
    TurnContinue,
    VictoryEpilogue,
    VictoryParade,
    VictoryCredits,
    FleetEncounter,
    FleetMove,
    FleetSelect,
    FleetList,
    FleetInspect,
    BoardConfirmation,
    FleetConstruction,
    PlunderConfirmation,
    Tribute,
    TributePolicy,
    ProvinceList,
    Menu,
    About,
    SaveSlots,
    LoadSlots,
    SaveDescription,
    Difficulty,
    Notice,
}

impl Phase {
    pub fn is_startup(self) -> bool {
        matches!(
            self,
            Self::Intro | Self::Title | Self::Credits | Self::Prologue
        )
    }

    pub fn parse(name: &str) -> Result<Self> {
        match name {
            "intro" => Ok(Self::Intro),
            "title" => Ok(Self::Title),
            "credits" => Ok(Self::Credits),
            "prologue" => Ok(Self::Prologue),
            "map" => Ok(Self::Map),
            "rome" => Ok(Self::Rome),
            "legions" => Ok(Self::Legions),
            "raise-legion" => Ok(Self::RaiseLegion),
            "legion-list" => Ok(Self::LegionList),
            "tribute" => Ok(Self::Tribute),
            "tribute-policy" => Ok(Self::TributePolicy),
            "province-list" => Ok(Self::ProvinceList),
            "menu" => Ok(Self::Menu),
            "about" => Ok(Self::About),
            _ => Err(format!("Unknown phase {name}")),
        }
    }
}

mod input;
pub use input::Input;

pub struct Session {
    pub phase: Phase,
    pub selected: usize,
    highlighted: bool,
    pub cursor: (i32, i32),
    drawn_cursor: (i32, i32),
    pub ticks: u32,
    pub phase_ticks: u32,
    pub rng: OriginalRng,
    pub last_random: u16,
    pub campaign: Campaign,
    pub selected_province: Option<u8>,
    inspected_region: Option<u8>,
    pub quit: bool,
    pub save_dir: Option<PathBuf>,
    pub tribute_indicator: bool,
    pub fleet_information: reforge_settings::FleetInformation,
    pub popup_cursor: reforge_settings::PopupCursor,
    pub menu_decorations: reforge_settings::MenuDecorations,
    menu_input: crate::state::ContinueInput,
    last_mouse: Option<(i32, i32)>,
    #[cfg(feature = "dev-tools")]
    pub slot_fallback: bool,
    #[cfg(feature = "dev-tools")]
    pub slot_errors: std::collections::BTreeMap<usize, String>,
    #[cfg(feature = "dev-tools")]
    recording: Option<crate::record::Recorder>,
    pub description: String,
    slot: usize,
    notice: String,
    intro: Bitmap,
    title: Bitmap,
    map: Bitmap,
    world: Vec<u8>,
    sprites: Vec<Bitmap>,
    intro_sequence: crate::intro::Intro,
    province_panels: crate::province::Panels,
    province_selection: Option<usize>,
    province: u8,
    province_header: Option<Frame>,
    list_page: usize,
    notice_return: Phase,
    legion_operation: usize,
    legion_menu_row: usize,
    legion_message: String,
    legion_message_background: Option<Frame>,
    legion_message_return: Phase,
    recruitment_payment: u16,
    moving_legion: Option<usize>,
    legion_province: u8,
    legion_selection_on_map: bool,
    diplomacy: Option<crate::diplomacy::Diplomacy>,
    battle: Option<crate::battle::Preparation>,
    encounter: Option<crate::encounter::Encounter>,
    construction: Option<crate::fleet::Construction>,
    boarding: Option<crate::fleet::Boarding>,
    moving_fleet: Option<usize>,
    fleet_region: u8,
    turn_income: u16,
    turn_stage: Option<u8>,
    naval: Option<crate::naval::Naval>,
    games: Option<crate::games::Games>,
    victory_parade: Option<crate::race_gallery::Parade>,
    victory_credits: Option<crate::ending::Credits>,
    revolt: Option<crate::turn::Revolt>,
    enemies: Option<crate::enemy::EnemyTurn>,
    enemy_background: Option<Frame>,
    revolt_background: Option<Frame>,
    fleet_destination: Option<u8>,
    fleet_army: Option<usize>,
    fleet_background: Option<Frame>,
    battle_lead_in: u32,
    pub skip_battle_fanfare: bool,
}

impl Session {
    pub fn new(assets: &Assets, phase: Phase, first: u16, second: u16) -> Result<Self> {
        let world = assets.resource("MINIT", "WORLD.LZW")?;
        if world.len() != 64_000 {
            return Err("Invalid world framebuffer".into());
        }
        let campaign = Campaign::new(assets)?;
        if phase.is_startup() {
            campaign.sound.play(0);
        } else if phase == Phase::Map {
            campaign.sound.play(1);
        }
        let selected = match phase {
            Phase::Tribute => campaign.tribute(0)? as usize,
            Phase::TributePolicy => campaign.data[0x5fc0] as usize,
            _ => 0,
        };
        let mut session = Self {
            phase,
            selected,
            highlighted: false,
            cursor: (160, 100),
            drawn_cursor: (160, 100),
            ticks: 0,
            phase_ticks: 0,
            rng: OriginalRng { first, second },
            last_random: 0,
            campaign,
            selected_province: matches!(
                phase,
                Phase::Rome | Phase::Tribute | Phase::TributePolicy | Phase::ProvinceList
            )
            .then_some(0),
            inspected_region: None,
            quit: false,
            save_dir: None,
            tribute_indicator: true,
            fleet_information: reforge_settings::FleetInformation::default(),
            popup_cursor: reforge_settings::PopupCursor::default(),
            menu_decorations: reforge_settings::MenuDecorations::default(),
            menu_input: crate::state::ContinueInput::default(),
            last_mouse: None,
            #[cfg(feature = "dev-tools")]
            slot_fallback: true,
            #[cfg(feature = "dev-tools")]
            slot_errors: std::collections::BTreeMap::new(),
            #[cfg(feature = "dev-tools")]
            recording: None,
            description: String::new(),
            slot: 0,
            notice: String::new(),
            intro: assets.bitmap("MINIT", "ROMULUS.CBM")?,
            title: assets.bitmap("MINIT", "TITLE.CBM")?,
            map: assets.cached_bitmap("MMAP", "MAP.CBM")?,
            world,
            sprites: assets.sprites("MINIT", "INIT")?,
            intro_sequence: crate::intro::Intro::load(assets)?,
            province_panels: crate::province::Panels::load(assets)?,
            province_selection: (phase == Phase::ProvinceList).then_some(5),
            province: 0,
            province_header: None,
            list_page: 0,
            notice_return: Phase::Map,
            legion_operation: 0,
            legion_menu_row: 0,
            legion_message: String::new(),
            legion_message_background: None,
            legion_message_return: Phase::Legions,
            recruitment_payment: 0,
            moving_legion: None,
            legion_province: 0,
            legion_selection_on_map: false,
            diplomacy: None,
            battle: None,
            encounter: None,
            construction: None,
            boarding: None,
            moving_fleet: None,
            fleet_region: 32,
            turn_income: 0,
            turn_stage: None,
            naval: None,
            games: None,
            victory_parade: None,
            victory_credits: None,
            revolt: None,
            enemies: None,
            enemy_background: None,
            revolt_background: None,
            fleet_destination: None,
            fleet_army: None,
            fleet_background: None,
            battle_lead_in: 0,
            skip_battle_fanfare: false,
        };
        session.set_menu_decorations(reforge_settings::MenuDecorations::default());
        Ok(session)
    }

    fn rows(&self) -> usize {
        match self.phase {
            Phase::Menu | Phase::Rome => 7,
            Phase::Naval => self
                .naval
                .as_ref()
                .filter(|s| s.stage == crate::naval::Stage::Ship)
                .map_or(0, |s| s.choices.len()),
            Phase::FleetSelect => self.campaign.local_fleets(self.fleet_region).len(),
            Phase::FleetConstruction
                if self
                    .construction
                    .as_ref()
                    .is_some_and(|s| s.purchase.is_none()) =>
            {
                4
            }
            Phase::BattlePreparation => self.battle.as_ref().map_or(0, |scene| match scene.stage {
                crate::battle::Stage::Formation => 4,
                crate::battle::Stage::Tactics => 5,
                crate::battle::Stage::Deployment => 0,
            }),
            Phase::Diplomacy => self.diplomacy.as_ref().map_or(0, |scene| {
                if scene.stage == crate::diplomacy::Stage::Choice {
                    scene.choices.len()
                } else {
                    0
                }
            }),
            Phase::Legions => 6,
            Phase::RaiseLegion => 4,
            Phase::LegionSelect => self
                .campaign
                .local_legions(self.legion_province)
                .len()
                .min(3),
            Phase::PlunderConfirmation | Phase::BoardConfirmation => 2,
            Phase::Tribute | Phase::TributePolicy => 6,
            Phase::SaveSlots | Phase::LoadSlots => 9,
            Phase::Difficulty => 4,
            _ => 0,
        }
    }

    fn battle_pointer_visible(&self) -> bool {
        self.phase != Phase::BattlePreparation
            || self.battle.as_ref().is_none_or(|scene| {
                scene.stage != crate::battle::Stage::Deployment
                    || (scene.motion.paused && scene.motion.message.is_none())
            })
    }

    #[cfg(feature = "sdl")]
    pub fn relative_steering(&self) -> bool {
        self.phase == Phase::Games && self.games.as_ref().is_some_and(|g| g.relative_steering())
    }

    #[cfg(feature = "dev-tools")]
    pub fn start_recording(&mut self, assets: &Assets, root: &Path, mute: bool) -> Result<()> {
        self.recording = Some(crate::record::Recorder::start(root, assets, self, mute)?);
        Ok(())
    }
    #[cfg(all(feature = "dev-tools", feature = "sdl"))]
    pub fn recording_active(&self) -> bool {
        self.recording.is_some()
    }
    #[cfg(feature = "dev-tools")]
    pub fn finish_recording(&mut self, assets: &Assets) -> Result<()> {
        if let Some(mut recorder) = self.recording.take()
            && let Err(error) = recorder.finish(assets, self)
        {
            recorder.fail(&error);
            return Err(error);
        }
        Ok(())
    }
    #[cfg(feature = "dev-tools")]
    pub fn record_scene(&self) -> String {
        let detail = self
            .games
            .as_ref()
            .map(|s| s.diagnostic())
            .or_else(|| self.naval.as_ref().map(|s| s.diagnostic()))
            .or_else(|| self.diplomacy.as_ref().map(|s| s.diagnostic()))
            .unwrap_or_default();
        format!(
            "{:?}:{}:{}:{}",
            self.phase,
            self.province,
            self.battle
                .as_ref()
                .map(|b| format!("{:?}", b.stage))
                .unwrap_or_default(),
            detail.split_whitespace().next().unwrap_or("")
        )
    }
    #[cfg(feature = "dev-tools")]
    pub fn record_context(&self) -> String {
        format!(
            "{} selection={} province={} cursor={},{} drawn={},{} rng={:04x},{:04x}",
            self.record_scene(),
            self.selected,
            self.province,
            self.cursor.0,
            self.cursor.1,
            self.drawn_cursor.0,
            self.drawn_cursor.1,
            self.rng.first,
            self.rng.second
        )
    }
    #[cfg(feature = "dev-tools")]
    pub fn record_slots(&self, assets: &Assets) -> Vec<Result<Vec<u8>>> {
        (0..9).map(|slot| self.load_slot(assets, slot)).collect()
    }
    #[cfg(feature = "dev-tools")]
    pub fn record_files(&self) -> Vec<(String, Vec<u8>)> {
        let mut files = vec![
            ("campaign.CSV".into(), self.campaign.save()),
            ("campaign.ds".into(), self.campaign.data.clone()),
        ];
        if let Some(scene) = &self.naval {
            files.push(("naval.ds".into(), scene.view.data.clone()));
        }
        if let Some(scene) = &self.battle {
            files.push((
                "units.bin".into(),
                scene.units.iter().flatten().copied().collect(),
            ));
            files.push(("grid.bin".into(), scene.grid().to_vec()));
        }
        if let Some(frame) = self.games.as_ref().and_then(|g| g.race_field_frame()) {
            files.push(("race-field.idx".into(), frame.pixels.clone()));
        }
        files
    }
    #[cfg(feature = "dev-tools")]
    pub fn input(&mut self, assets: &Assets, input: Input) -> Result<()> {
        // Take the recorder so recursive input translations remain one event.
        let Some(mut recorder) = self.recording.take() else {
            return self.apply_input(assets, input);
        };
        let before = (self.ticks, self.record_context());
        if let Err(error) = recorder.before_input(&input) {
            recorder.fail(&error);
            eprintln!("Playthrough recording stopped: {error}. The recording is incomplete.");
            return self.apply_input(assets, input);
        }
        crate::record::begin_trace();
        let result = self.apply_input(assets, input.clone());
        let traces = crate::record::end_trace();
        match recorder.event(
            assets,
            self,
            &input,
            &before,
            traces,
            result.as_ref().err().map(String::as_str),
        ) {
            Ok(()) => self.recording = Some(recorder),
            Err(error) => {
                recorder.fail(&error);
                eprintln!("Playthrough recording stopped: {error}. The recording is incomplete.");
            }
        }
        result
    }
    #[cfg(not(feature = "dev-tools"))]
    pub fn input(&mut self, assets: &Assets, input: Input) -> Result<()> {
        self.apply_input(assets, input)
    }
    pub fn set_menu_decorations(&mut self, choice: reforge_settings::MenuDecorations) {
        self.menu_decorations = choice;
        self.sync_menu_decorations();
    }
    fn sync_menu_decorations(&mut self) {
        let keyboard = match self.menu_decorations {
            reforge_settings::MenuDecorations::Mouse => false,
            reforge_settings::MenuDecorations::Keyboard => true,
            reforge_settings::MenuDecorations::Dynamic => {
                self.menu_input == crate::state::ContinueInput::Keyboard
            }
        };
        self.campaign.keyboard_menus = keyboard;
        if let Some(scene) = self.naval.as_mut() {
            scene.view.keyboard_menus = keyboard;
        }
    }
    fn apply_input(&mut self, assets: &Assets, input: Input) -> Result<()> {
        #[cfg(feature = "dev-tools")]
        if matches!(input, Input::Mark(_)) {
            return Ok(());
        }
        let menu_input = input.menu_input(self.last_mouse).unwrap_or(self.menu_input);
        let last_mouse = match input {
            Input::Mouse(x, y) | Input::Click(x, y) | Input::RightClick(x, y) => Some((x, y)),
            _ => self.last_mouse,
        };
        self.menu_input = menu_input;
        self.last_mouse = last_mouse;
        self.sync_menu_decorations();
        let continue_input = input.continue_input();
        if let Some(method) = continue_input {
            self.campaign.continue_input = method;
            if let Some(scene) = self.naval.as_mut() {
                scene.view.continue_input = method;
            }
        }
        let before = self.phase;
        let was_visible = self.battle_pointer_visible();
        let timer = matches!(input, Input::Tick(_));
        self.input_inner(assets, input)?;
        // Space can dispatch a synthetic click, and loading can replace state.
        self.menu_input = menu_input;
        self.last_mouse = last_mouse;
        self.sync_menu_decorations();
        // New encounters or campaign resets can replace the cloned state.
        if let Some(method) = continue_input {
            self.campaign.continue_input = method;
            if let Some(scene) = self.naval.as_mut() {
                scene.view.continue_input = method;
            }
        }
        if self.phase != Phase::BattlePreparation
            || (was_visible || timer) && self.battle_pointer_visible()
        {
            self.drawn_cursor = self.cursor;
        }
        if !before.is_startup() && self.phase.is_startup() {
            self.campaign.sound.play(0);
        }
        if before == Phase::Diplomacy && self.phase != Phase::Diplomacy {
            self.campaign.sound.stop();
        }
        if before.is_startup() && !self.phase.is_startup() || self.quit {
            self.campaign.sound.stop();
        }
        if self.phase == Phase::Map && before != Phase::Map && !self.quit {
            self.campaign.sound.play(1);
        }
        if before == Phase::Map
            && matches!(
                self.phase,
                Phase::Menu | Phase::Rome | Phase::LegionMove | Phase::FleetSelect
            )
        {
            self.campaign.sound.stop();
        }
        Ok(())
    }

    fn input_inner(&mut self, assets: &Assets, input: Input) -> Result<()> {
        #[cfg(feature = "dev-tools")]
        if matches!(input, Input::Mark(_)) {
            return Ok(());
        }
        if matches!(input, Input::Quit) {
            self.quit = true;
            return Ok(());
        }
        if self.phase == Phase::VictoryCredits {
            let finished = if let Input::Tick(count) = input {
                self.advance_time(count);
                self.victory_credits
                    .as_mut()
                    .ok_or("Missing ending credits")?
                    .tick(count)
            } else if matches!(
                input,
                Input::Mouse(_, _) | Input::Release(_, _) | Input::ReleaseKey(_) | Input::Random(_)
            ) {
                false
            } else {
                self.victory_credits
                    .as_mut()
                    .ok_or("Missing ending credits")?
                    .skip()
            };
            if finished {
                self.victory_credits = None;
                self.reset_after_victory(assets)?;
            }
            return Ok(());
        }
        if self.phase == Phase::VictoryParade {
            let finished = if let Input::Tick(count) = input {
                self.advance_time(count);
                self.victory_parade
                    .as_mut()
                    .ok_or("Missing victory parade")?
                    .tick(&mut self.campaign, &mut self.rng, count)?
            } else {
                !matches!(
                    input,
                    Input::Mouse(_, _)
                        | Input::Release(_, _)
                        | Input::ReleaseKey(_)
                        | Input::Random(_)
                )
            };
            if finished {
                let background = self.victory_parade.as_ref().unwrap().render();
                let unused = self.victory_parade.as_ref().unwrap().unspent_ticks();
                let mut credits = crate::ending::Credits::new(assets, &self.campaign, background)?;
                let done = credits.tick(unused);
                self.victory_credits = Some(credits);
                self.victory_parade = None;
                self.phase = Phase::VictoryCredits;
                self.phase_ticks = 0;
                self.highlighted = false;
                if done {
                    self.victory_credits = None;
                    self.reset_after_victory(assets)?;
                }
            }
            return Ok(());
        }
        if self.phase == Phase::Games {
            let mut input = input;
            if self.games.as_ref().is_some_and(|s| s.moves_cursor()) {
                match input {
                    Input::Up => self.cursor.1 = (self.cursor.1 - 8).max(0),
                    Input::Down => self.cursor.1 = (self.cursor.1 + 8).min(199),
                    Input::Left => self.cursor.0 = (self.cursor.0 - 8).max(0),
                    Input::Right => self.cursor.0 = (self.cursor.0 + 8).min(319),
                    Input::Space | Input::Enter => {
                        input = Input::Click(self.cursor.0, self.cursor.1)
                    }
                    _ => {}
                }
            }
            if let Input::Mouse(x, y) | Input::Click(x, y) = input {
                self.cursor = (x, y);
            }
            if let Input::Tick(count) = input {
                self.advance_time(count);
            }
            let scene = self.games.as_mut().ok_or("Missing games scene")?;
            if scene.input(
                assets,
                &mut self.campaign,
                &mut self.rng,
                &self.province_panels,
                input,
            )? {
                self.games = None;
                self.close_province_modal();
            }
            return Ok(());
        }
        if matches!(input, Input::ReleaseKey(_)) && self.phase != Phase::Naval {
            return Ok(());
        }
        if self.phase == Phase::Naval {
            let stage = self.naval.as_ref().ok_or("Missing naval scene")?.stage;
            use crate::naval::Stage as N;
            if let Input::Tick(count) = input {
                self.advance_time(count);
                self.naval.as_mut().unwrap().advance(
                    assets,
                    count,
                    &mut self.campaign,
                    &mut self.rng,
                )?;
                if stage != N::Reengage && self.naval.as_ref().unwrap().stage == N::Reengage {
                    self.selected = 1;
                    self.highlighted = true;
                }
                return Ok(());
            }
            let action = !matches!(
                input,
                Input::Mouse(..) | Input::Release(..) | Input::ReleaseKey(_) | Input::Random(_)
            );
            if stage == N::Briefing && action {
                self.naval
                    .as_mut()
                    .unwrap()
                    .begin_round(&mut self.campaign)?;
                return Ok(());
            }
            if stage == N::Report && action {
                self.finish_naval(assets)?;
                return Ok(());
            }
            if stage == N::BoardingReport && action {
                self.naval.as_mut().unwrap().acknowledge_boarding(
                    assets,
                    &mut self.campaign,
                    &mut self.rng,
                )?;
                return Ok(());
            }
            if stage == N::Reengage {
                if let Input::Mouse(x, y) | Input::Click(x, y) = input {
                    self.cursor = (x.clamp(0, 319), y.clamp(0, 199));
                }
                let yes = match input {
                    Input::Up | Input::Down | Input::Left | Input::Right => {
                        self.selected ^= 1;
                        None
                    }
                    Input::Shortcut(b'y' | b'Y') => Some(true),
                    Input::Shortcut(b'n' | b'N') | Input::Escape => Some(false),
                    Input::Enter | Input::Space => Some(self.selected == 0),
                    Input::Click(x, y) if (112..120).contains(&y) && (129..160).contains(&x) => {
                        Some(true)
                    }
                    Input::Click(x, y) if (112..120).contains(&y) && (165..197).contains(&x) => {
                        Some(false)
                    }
                    _ => None,
                };
                if let Some(yes) = yes {
                    self.naval.as_mut().unwrap().reengage(
                        assets,
                        yes,
                        &mut self.campaign,
                        &mut self.rng,
                    )?;
                }
                return Ok(());
            }
            if stage == N::Battle {
                let scene = self.naval.as_mut().unwrap();
                match input {
                    Input::Up => scene.held |= 8,
                    Input::Down => scene.held |= 4,
                    Input::Left => scene.held |= 2,
                    Input::Right => scene.held |= 1,
                    Input::ReleaseKey(bits) => scene.held &= !bits,
                    Input::Space => scene.fire_arrows(),
                    Input::Enter | Input::Shortcut(b'+') => scene.board_enemy(),
                    Input::Shortcut(b'r' | b'R') => {
                        scene.retreat(assets, &mut self.campaign, &mut self.rng)?
                    }
                    _ => {}
                }
                return Ok(());
            }
            if matches!(stage, N::Boarding | N::Animation) {
                return Ok(());
            }
            if stage == N::Ship && matches!(input, Input::Escape | Input::RightClick(..)) {
                self.naval = None;
                if self.enemies.is_some() {
                    return self.finish_enemy_naval(assets, 0);
                }
                self.moving_fleet = None;
                self.fleet_destination = None;
                self.fleet_army = None;
                self.fleet_background = None;
                self.phase = Phase::Map;
                return Ok(());
            }
        }
        if self.phase == Phase::Map
            && (matches!(input, Input::Shortcut(b'e' | b'E'))
                || matches!(input, Input::Click(x,y) if (227..=275).contains(&x) && (2..=18).contains(&y)))
        {
            self.turn_income = 0;
            self.campaign.turn_recovery()?;
            self.turn_stage = Some(0);
            self.phase = Phase::TurnContinue;
            return self.advance_turn(assets);
        }
        if let Input::TurnCheck(step) = input {
            match step {
                255 => {
                    self.turn_income = 0;
                    self.campaign.turn_recovery()?;
                }
                254 => self.campaign.turn_finish_year(self.turn_income)?,
                253 => {
                    self.enemies = Some(crate::enemy::EnemyTurn::new(&self.campaign)?);
                    self.phase = Phase::TurnEnemy;
                    return self.advance_enemies(assets);
                }
                province if province < 27 => {
                    if self.campaign.turn_province_economy(province)? {
                        self.revolt_background = Some(crate::campaign::render(
                            assets,
                            &self.campaign,
                            &self.map,
                            &self.sprites,
                        )?);
                        self.revolt =
                            Some(crate::turn::Revolt::begin(&mut self.campaign, province)?);
                        self.phase = Phase::TurnRevolt;
                        self.advance_revolt(assets)?;
                        return Ok(());
                    }
                    self.turn_income = self
                        .turn_income
                        .wrapping_add(self.campaign.turn_province_finish(province)?);
                }
                _ => return Err("Invalid turn diagnostic step".into()),
            }
            return Ok(());
        }
        if self.phase == Phase::RegionInspect
            && matches!(
                input,
                Input::Enter
                    | Input::Space
                    | Input::Escape
                    | Input::Backspace
                    | Input::Text(_)
                    | Input::Shortcut(_)
                    | Input::Click(_, _)
                    | Input::RightClick(_, _)
            )
        {
            self.phase = Phase::Map;
            self.inspected_region = None;
            self.highlighted = false;
            return Ok(());
        }
        if matches!(self.phase, Phase::FleetList | Phase::FleetInspect)
            && !matches!(
                input,
                Input::Tick(_) | Input::Mouse(_, _) | Input::Release(_, _) | Input::Random(_)
            )
        {
            self.phase = Phase::Map;
            self.highlighted = false;
            return Ok(());
        }
        if self.phase == Phase::BoardConfirmation && matches!(input, Input::Escape) {
            self.boarding = None;
            self.legion_message_background = None;
            self.phase = Phase::LegionMove;
            self.highlighted = false;
            return Ok(());
        }
        if self.phase == Phase::FleetConstruction {
            let scene = self.construction.as_mut().ok_or("Missing construction")?;
            if scene.purchase.is_some() {
                if let Input::Click(x, y) | Input::Mouse(x, y) = &input {
                    self.cursor = ((*x).clamp(0, 319), (*y).clamp(0, 199));
                }
                match input {
                    Input::Tick(count) => self.advance_time(count),
                    Input::Up => scene.adjust(&self.campaign, true),
                    Input::Down => scene.adjust(&self.campaign, false),
                    Input::Escape | Input::Shortcut(b'c' | b'C') => {
                        scene.finish_purchase(&mut self.campaign, false)?;
                        self.highlighted = false;
                    }
                    Input::Enter | Input::Space | Input::Shortcut(b'+' | b'o' | b'O') => {
                        scene.finish_purchase(&mut self.campaign, true)?;
                        self.highlighted = false;
                    }
                    Input::Click(x, y) if (90..99).contains(&x) && (81..88).contains(&y) => {
                        scene.adjust(&self.campaign, true)
                    }
                    Input::Click(x, y) if (90..99).contains(&x) && (91..98).contains(&y) => {
                        scene.adjust(&self.campaign, false)
                    }
                    Input::Click(x, y) if (90..159).contains(&x) && (103..114).contains(&y) => {
                        scene.finish_purchase(&mut self.campaign, true)?;
                        self.highlighted = false;
                    }
                    Input::Click(x, y) if (160..230).contains(&x) && (103..114).contains(&y) => {
                        scene.finish_purchase(&mut self.campaign, false)?;
                        self.highlighted = false;
                    }
                    Input::Mouse(x, y) => self.cursor = (x.clamp(0, 319), y.clamp(0, 199)),
                    _ => {}
                }
                return Ok(());
            }
            if matches!(input, Input::Escape) {
                scene.finish(&mut self.campaign)?;
                self.construction = None;
                self.close_province_modal();
                self.highlighted = false;
                return Ok(());
            }
        }
        if self.phase == Phase::LegionMessage
            && matches!(
                input,
                Input::Enter
                    | Input::Escape
                    | Input::Shortcut(_)
                    | Input::Backspace
                    | Input::Up
                    | Input::Down
                    | Input::Left
                    | Input::Right
                    | Input::Click(_, _)
                    | Input::RightClick(_, _)
            )
        {
            self.campaign.finish_recruitment(self.recruitment_payment)?;
            self.recruitment_payment = 0;
            self.legion_message_background = None;
            self.phase = self.legion_message_return;
            self.selected = if self.phase == Phase::Legions {
                self.legion_menu_row
            } else {
                self.selected
            };
            self.highlighted = true;
            if self.phase == Phase::Rome {
                let building =
                    self.province_selection == Some(if self.province == 0 { 3 } else { 4 });
                if self.province != 0 && self.province_selection == Some(1) {
                    self.province_selection = Some(0);
                }
                self.close_province_modal();
                if building {
                    self.highlighted = false;
                }
            }
            if self.phase == Phase::Title {
                self.campaign = Campaign::new(assets)?;
                self.selected_province = None;
                self.phase_ticks = 0;
                self.highlighted = false;
            }
            if self.phase == Phase::VictoryEpilogue {
                return self.show_legion_message(
                    assets,
                    crate::ending::epilogue(&self.campaign)?,
                    Phase::VictoryParade,
                );
            }
            if self.phase == Phase::VictoryParade {
                self.victory_parade = Some(crate::race_gallery::Parade::new(
                    assets,
                    &self.campaign,
                    &mut self.rng,
                    true,
                )?);
            }
            if self.phase == Phase::TurnContinue {
                self.advance_turn(assets)?;
            }
            if self.phase == Phase::TurnEnemy {
                self.advance_enemies(assets)?;
            }
            if self.phase == Phase::TurnRevolt {
                self.advance_revolt(assets)?;
            }
            if self.phase == Phase::FleetEncounter {
                self.fight_naval_army(assets)?;
            }
            if self.phase == Phase::Encounter {
                self.advance_encounter(assets)?;
            }
            return Ok(());
        }
        if self.phase == Phase::Diplomacy {
            let stage = self.diplomacy.as_ref().ok_or("Missing conversation")?.stage;
            if matches!(
                stage,
                crate::diplomacy::Stage::CleoAnimation
                    | crate::diplomacy::Stage::CleoAfter
                    | crate::diplomacy::Stage::CleoPause
                    | crate::diplomacy::Stage::CleoFinal
            ) && matches!(input, Input::Up | Input::Down | Input::Left | Input::Right)
            {
                return Ok(());
            }
            if stage != crate::diplomacy::Stage::Choice
                && matches!(
                    input,
                    Input::Enter
                        | Input::Space
                        | Input::Escape
                        | Input::Shortcut(_)
                        | Input::Click(_, _)
                        | Input::RightClick(_, _)
                        | Input::Backspace
                        | Input::Up
                        | Input::Down
                        | Input::Left
                        | Input::Right
                )
            {
                self.advance_diplomacy(assets)?;
                return Ok(());
            }
        }
        if self.phase == Phase::BattlePreparation
            && let Some(scene) = self.battle.as_mut()
            && scene.stage == crate::battle::Stage::Deployment
        {
            if self.battle_lead_in != 0 && !matches!(input, Input::Tick(_) | Input::Random(_)) {
                return Ok(());
            }
            if scene.outcome.is_some()
                && !matches!(
                    input,
                    Input::Tick(_) | Input::Mouse(_, _) | Input::Release(_, _) | Input::Random(_)
                )
            {
                if scene.acknowledge_outcome(&mut self.campaign, &mut self.rng)? {
                    let victory = scene.outcome.as_ref().unwrap().victory;
                    self.complete_land_battle(assets, victory)?;
                }
                return Ok(());
            }
            if let Input::Click(x, y)
            | Input::Mouse(x, y)
            | Input::RightClick(x, y)
            | Input::Release(x, y) = &input
            {
                self.cursor = ((*x).clamp(0, 319), (*y).clamp(0, 199));
            }
            if scene.motion.paused && scene.motion.selected.is_none() && !scene.motion.show_orders {
                match input {
                    Input::Up => self.cursor.1 = (self.cursor.1 - 8).max(0),
                    Input::Down => self.cursor.1 = (self.cursor.1 + 8).min(199),
                    Input::Left => self.cursor.0 = (self.cursor.0 - 8).max(0),
                    Input::Right => self.cursor.0 = (self.cursor.0 + 8).min(319),
                    _ => {}
                }
            }
            if scene.control(&input, &self.campaign, self.cursor)? {
                return Ok(());
            }
        }
        match input {
            Input::Shortcut(key) if self.phase == Phase::BattlePreparation && self.rows() != 0 => {
                let menu = self
                    .battle
                    .as_ref()
                    .ok_or("Missing battle")?
                    .menu(&self.campaign)?;
                if let Some(row) = menu
                    .items
                    .iter()
                    .position(|item| item.shortcut == key.to_ascii_lowercase())
                {
                    self.selected = row;
                    self.highlighted = true;
                    self.activate(assets)?;
                }
            }
            Input::Escape if self.phase == Phase::BattlePreparation => {
                self.complete_land_battle(assets, false)?;
            }
            Input::Shortcut(key) if self.phase == Phase::Diplomacy => {
                let scene = self.diplomacy.as_ref().ok_or("Missing conversation")?;
                if let Some(row) = scene
                    .choices
                    .iter()
                    .position(|choice| choice.shortcut == key.to_ascii_lowercase())
                {
                    self.selected = row;
                    self.highlighted = true;
                    self.activate(assets)?;
                }
            }
            Input::Escape if self.phase == Phase::Diplomacy => {}
            Input::Space if self.phase == Phase::Map => {
                self.input(assets, Input::Click(self.cursor.0, self.cursor.1))?;
            }
            Input::Space => self.input(assets, Input::Enter)?,
            Input::RightClick(x, y) if self.phase == Phase::Map => {
                self.cursor = (x.clamp(0, 319), y.clamp(0, 185));
                self.start_map_march(assets)?;
            }
            Input::Enter if self.phase == Phase::Map => self.start_map_march(assets)?,
            Input::RightClick(_, _) if self.phase == Phase::LegionMove => {
                self.finish_march(None)?;
            }
            Input::Escape if self.phase == Phase::LegionMove => {
                self.finish_march(None)?;
            }
            Input::Up | Input::Down | Input::Left | Input::Right
                if matches!(
                    self.phase,
                    Phase::Map | Phase::LegionMove | Phase::FleetMove
                ) =>
            {
                match input {
                    Input::Up => self.cursor.1 = (self.cursor.1 - 8).max(0),
                    Input::Down => self.cursor.1 = (self.cursor.1 + 8).min(185),
                    Input::Left => self.cursor.0 = (self.cursor.0 - 8).max(0),
                    Input::Right => self.cursor.0 = (self.cursor.0 + 8).min(311),
                    _ => unreachable!(),
                }
            }
            Input::Enter if self.phase == Phase::FleetMove => {
                self.choose_fleet_destination(assets)?
            }
            Input::Escape | Input::RightClick(_, _)
                if matches!(self.phase, Phase::FleetMove | Phase::FleetSelect) =>
            {
                self.phase = Phase::Map;
                self.moving_fleet = None;
            }
            Input::Enter if self.phase == Phase::LegionMove => {
                self.choose_march_destination(assets)?;
            }
            Input::Tick(count) => {
                self.advance_time(count);
                if self.phase == Phase::BattlePreparation {
                    let consumed = count.min(self.battle_lead_in);
                    let waiting = self.battle_lead_in != 0;
                    self.battle_lead_in -= consumed;
                    if waiting && self.battle_lead_in == 0 {
                        self.campaign
                            .sound
                            .play(self.battle.as_ref().ok_or("Missing battle")?.battle_music());
                    }
                    self.battle.as_mut().ok_or("Missing battle")?.advance(
                        count - consumed,
                        &mut self.campaign,
                        &mut self.rng,
                    )?;
                }
                if self.phase == Phase::Diplomacy {
                    let before = self.diplomacy.as_ref().unwrap().stage;
                    self.diplomacy.as_mut().unwrap().tick(
                        assets,
                        &mut self.campaign,
                        &mut self.rng,
                        count,
                    )?;
                    let after = self.diplomacy.as_ref().unwrap().stage;
                    if after == crate::diplomacy::Stage::Complete
                        || (before == crate::diplomacy::Stage::Withdraw
                            && self.diplomacy.as_ref().unwrap().response_ticks >= 72)
                    {
                        self.advance_diplomacy(assets)?;
                    } else if after == crate::diplomacy::Stage::Choice && before != after {
                        self.selected = 0;
                        self.highlighted = false;
                    }
                }
            }
            Input::Random(bound) => self.last_random = self.rng.bounded(bound),
            Input::Text(text) if self.phase == Phase::SaveDescription => {
                for character in text
                    .chars()
                    .filter(|c| c.is_ascii() && !c.is_ascii_control())
                {
                    if self.description.len() < 47 {
                        self.description.push(character);
                    }
                }
            }
            Input::Backspace if self.phase == Phase::SaveDescription => {
                self.description.pop();
            }
            Input::Shortcut(_) | Input::Up | Input::Down | Input::Backspace
                if self.phase.is_startup() =>
            {
                self.advance()
            }
            Input::Shortcut(_) if matches!(self.phase, Phase::ProvinceList | Phase::LegionList) => {
                self.advance()
            }
            Input::Up | Input::Down | Input::Left | Input::Right | Input::Backspace
                if self.phase == Phase::LegionList =>
            {
                self.advance()
            }
            Input::Escape if self.phase == Phase::PlunderConfirmation => {
                self.province_selection = Some(0);
                self.close_province_modal()
            }
            Input::Left | Input::Right
                if matches!(
                    self.phase,
                    Phase::PlunderConfirmation | Phase::BoardConfirmation
                ) =>
            {
                self.selected ^= 1;
                self.highlighted = true;
            }
            Input::Shortcut(key)
                if matches!(
                    self.phase,
                    Phase::PlunderConfirmation | Phase::BoardConfirmation
                ) =>
            {
                if let Some(row) = b"yn".iter().position(|b| *b == key.to_ascii_lowercase()) {
                    self.selected = row;
                    self.activate(assets)?;
                }
            }
            Input::Shortcut(key) if self.phase == Phase::Naval && self.rows() != 0 => {
                let menu = self.naval.as_ref().unwrap().menu()?;
                if let Some(row) = menu
                    .items
                    .iter()
                    .position(|item| item.shortcut == key.to_ascii_lowercase())
                {
                    self.selected = row;
                    self.activate(assets)?;
                }
            }
            Input::Shortcut(key) if self.phase == Phase::Map && key.eq_ignore_ascii_case(&b'f') => {
                if !self.campaign.fleets().is_empty() {
                    self.phase = Phase::FleetList;
                }
            }
            Input::Shortcut(key) if self.phase == Phase::FleetSelect => {
                if (b'1'..=b'3').contains(&key) && usize::from(key - b'1') < self.rows() {
                    self.selected = usize::from(key - b'1');
                    self.activate(assets)?;
                }
            }
            Input::Shortcut(key) if self.phase == Phase::LegionSelect => {
                if (b'1'..=b'3').contains(&key) && usize::from(key - b'1') < self.rows() {
                    self.selected = usize::from(key - b'1');
                    self.activate(assets)?;
                }
            }
            Input::Shortcut(key) if self.province_menu().is_some() => {
                let menu =
                    crate::province::Menu::read(&self.campaign, self.province_menu().unwrap())?;
                if let Some(row) = menu
                    .items
                    .iter()
                    .position(|item| item.shortcut == key.to_ascii_lowercase() && !item.disabled)
                {
                    self.selected = row;
                    self.highlighted = true;
                    self.activate(assets)?;
                }
            }
            Input::Shortcut(key) if self.phase == Phase::Menu => {
                if let Some(row) = b"slnadqc"
                    .iter()
                    .position(|b| *b == key.to_ascii_lowercase())
                {
                    self.selected = row;
                    self.activate(assets)?;
                }
            }
            Input::Mouse(x, y) | Input::Click(x, y) => {
                self.highlighted = false;
                self.cursor = (x.clamp(0, 319), y.clamp(0, 199));
                if self.phase == Phase::LegionMove {
                    self.cursor.0 = self.cursor.0.min(311);
                    self.cursor.1 = self.cursor.1.min(185);
                }
                let click = matches!(input, Input::Click(_, _));
                if click && self.phase == Phase::FleetMove {
                    self.choose_fleet_destination(assets)?;
                    return Ok(());
                }
                if click && self.phase == Phase::LegionMove {
                    self.choose_march_destination(assets)?;
                    return Ok(());
                }
                if click && self.phase == Phase::Map {
                    if let Some(region) = self.campaign.region_at(x, y)?
                        && region >= 27
                    {
                        self.open_fleets(assets, region)?;
                        return Ok(());
                    }
                    self.selected_province = self.campaign.province_at(x, y)?;
                    if let Some(province) = self.selected_province
                        && self.campaign.ownership(province)? == 0
                    {
                        self.inspected_region = Some(province);
                        self.phase = Phase::RegionInspect;
                        return Ok(());
                    }
                    if let Some(province) = self.selected_province
                        && self.campaign.ownership(province)? != 0
                    {
                        self.province = province;
                        self.province_header = if province == 0 {
                            None
                        } else {
                            Some(crate::province::Panels::header(
                                assets,
                                &self.campaign,
                                province,
                                &mut self.rng,
                            )?)
                        };
                        self.refresh_province_menu();
                        self.phase = Phase::Rome;
                        self.selected = 0;
                        self.province_selection = None;
                        // Consume the map click: its coordinates must not also
                        // activate a command in the newly opened panel.
                        return Ok(());
                    }
                }
                if let Some(row) = self.menu_hit(x, y) {
                    self.selected = row;
                    self.highlighted = self.province_menu().is_some()
                        || matches!(
                            self.phase,
                            Phase::Diplomacy | Phase::BattlePreparation | Phase::Naval
                        );
                    if click {
                        self.activate(assets)?;
                    }
                } else if click
                    && matches!(
                        self.phase,
                        Phase::Intro
                            | Phase::Title
                            | Phase::Credits
                            | Phase::Prologue
                            | Phase::About
                            | Phase::Notice
                            | Phase::ProvinceList
                            | Phase::LegionList
                    )
                {
                    self.advance();
                }
            }
            Input::Escape
                if matches!(
                    self.phase,
                    Phase::RaiseLegion | Phase::LegionSelect | Phase::LegionList
                ) =>
            {
                self.phase = if self.phase == Phase::LegionSelect && self.legion_selection_on_map {
                    Phase::Map
                } else {
                    Phase::Legions
                };
                self.selected = self.legion_menu_row;
                self.highlighted = true;
            }
            Input::Escape if self.phase == Phase::Legions => self.close_province_modal(),
            Input::Escape
                if matches!(
                    self.phase,
                    Phase::Rome | Phase::Tribute | Phase::TributePolicy | Phase::ProvinceList
                ) =>
            {
                if self.phase == Phase::Rome {
                    self.phase = Phase::Map;
                } else {
                    self.close_province_modal();
                }
            }
            Input::Escape if self.phase == Phase::Notice && self.notice_return == Phase::Rome => {
                self.advance()
            }
            Input::Escape => {
                if self.phase.is_startup() {
                    self.advance();
                    return Ok(());
                }
                self.selected = 0;
                self.highlighted = false;
                self.phase = if self.phase == Phase::Map {
                    Phase::Menu
                } else {
                    Phase::Map
                };
            }
            Input::Up if self.rows() != 0 => {
                self.highlighted = true;
                self.move_selection(false)?;
            }
            Input::Down if self.rows() != 0 => {
                self.highlighted = true;
                self.move_selection(true)?;
            }
            Input::Enter => {
                if self.rows() != 0 || self.phase == Phase::SaveDescription {
                    self.activate(assets)?;
                } else {
                    self.advance();
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn reset_after_victory(&mut self, assets: &Assets) -> Result<()> {
        self.campaign = Campaign::new(assets)?;
        self.selected_province = None;
        self.phase = Phase::Title;
        self.phase_ticks = 0;
        self.highlighted = false;
        self.intro_sequence.water.reset();
        self.campaign.sound.play(0);
        Ok(())
    }

    fn menu_hit(&self, x: i32, y: i32) -> Option<usize> {
        if self.phase == Phase::Naval && self.rows() != 0 {
            return self.naval.as_ref()?.menu().ok()?.hit(x, y);
        }
        if self.phase == Phase::FleetSelect {
            if !(8..=311).contains(&x) || y < 25 {
                return None;
            }
            let row = ((y - 25) / 10) as usize;
            return (row < self.rows()).then_some(row);
        }
        if matches!(
            self.phase,
            Phase::PlunderConfirmation | Phase::BoardConfirmation
        ) {
            let top = if self.phase == Phase::BoardConfirmation {
                106
            } else {
                101
            };
            return if (top..=top + 10).contains(&y) {
                if (129..=158).contains(&x) {
                    Some(0)
                } else if (165..=189).contains(&x) {
                    Some(1)
                } else {
                    None
                }
            } else {
                None
            };
        }
        if self.phase == Phase::BattlePreparation {
            if self.rows() == 0 {
                return None;
            }
            return self.battle.as_ref()?.menu(&self.campaign).ok()?.hit(x, y);
        }
        if self.phase == Phase::Diplomacy {
            if !(15..=101).contains(&x) || y < 155 {
                return None;
            }
            let row = ((y - 155) / 13) as usize;
            return (row < self.rows() && (y - 155) % 13 <= 10).then_some(row);
        }
        if self.phase == Phase::LegionSelect {
            if !(8..=311).contains(&x) || y < 24 {
                return None;
            }
            let row = ((y - 24) / 10) as usize;
            return (row < self.rows()).then_some(row);
        }
        if let Some(address) = self.province_menu() {
            return crate::province::Menu::read(&self.campaign, address)
                .ok()?
                .hit(x, y);
        }
        let (left, right, top, row_height) = if self.phase == Phase::Menu {
            (87, 220, 102, 10)
        } else {
            (40, 280, 68, 10)
        };
        if self.rows() == 0 || x < left || x > right || y < top {
            return None;
        }
        let row = ((y - top) / row_height) as usize;
        (row < self.rows()).then_some(row)
    }

    fn province_menu(&self) -> Option<usize> {
        match self.phase {
            Phase::Rome => Some(self.command_menu()),
            Phase::FleetConstruction
                if self
                    .construction
                    .as_ref()
                    .is_some_and(|s| s.purchase.is_none()) =>
            {
                Some(crate::fleet::MENU)
            }
            Phase::Legions => Some(crate::legion::MENU),
            Phase::RaiseLegion => Some(crate::legion::RAISE_MENU),
            Phase::Tribute => Some(crate::province::TRIBUTE_MENU),
            Phase::TributePolicy => Some(crate::province::POLICY_MENU),
            _ => None,
        }
    }

    fn open_fleets(&mut self, assets: &Assets, region: u8) -> Result<()> {
        self.fleet_region = region;
        let local = self.campaign.local_fleets(region);
        if local.is_empty() {
            if let Some(id) = (3..6).find(|i| {
                self.campaign.data[0x1114 + i * 12] == region
                    && self.campaign.data[0x1116 + i * 12] != 255
            }) {
                self.moving_fleet = Some(id);
                self.phase = Phase::FleetInspect;
            } else {
                self.inspected_region = Some(region);
                self.phase = Phase::RegionInspect;
            }
            return Ok(());
        }
        self.selected = 0;
        self.highlighted = false;
        if local.len() == 1 && self.campaign.data[0x1116 + local[0] * 12] == 0 {
            self.moving_fleet = Some(local[0]);
            self.phase = Phase::FleetMove;
        } else {
            self.phase = Phase::FleetSelect;
            if local
                .iter()
                .all(|i| self.campaign.data[0x1116 + i * 12] != 0)
            {
                self.show_legion_message(
                    assets,
                    self.campaign
                        .string(if local.len() == 1 { 0x1348 } else { 0x136a })?,
                    Phase::Map,
                )?;
            }
        }
        Ok(())
    }
    fn choose_fleet_destination(&mut self, assets: &Assets) -> Result<()> {
        let Some(target) = self.campaign.region_at(self.cursor.0, self.cursor.1)? else {
            return Ok(());
        };
        self.advance_fleet_destination(assets, target)
    }
    fn advance_fleet_destination(&mut self, assets: &Assets, target: u8) -> Result<()> {
        let index = self.moving_fleet.ok_or("Missing selected fleet")?;
        let source = self.campaign.fleet_location(index)?;
        if target == source {
            self.phase = Phase::Map;
            self.moving_fleet = None;
            return Ok(());
        }
        if !self.campaign.fleet_destinations(index)?.contains(&target) {
            return Ok(());
        }
        if self.fleet_destination.is_none()
            && !self.campaign.has_roaming_army(target)
            && !(3..6).any(|i| {
                self.campaign.data[0x1114 + i * 12] == target
                    && self.campaign.data[0x1116 + i * 12] != 255
            })
        {
            self.campaign.complete_fleet_move(index, target)?;
            self.cursor = self.campaign.region_coordinate(target)?;
            self.phase = Phase::Map;
            self.moving_fleet = None;
            return Ok(());
        }
        if self.fleet_destination != Some(target) {
            self.fleet_background = Some(self.render(assets, false)?);
            self.fleet_destination = Some(target);
            self.campaign.data[0x1116 + index * 12] = 1;
        }
        // The original resolves roaming armies before permanent enemy fleets.
        if let Some(army) = self.campaign.roaming_army(target, 0) {
            self.fleet_army = Some(army);
            let origin = word(&self.campaign.data, 0xc114 + army * 14)?;
            let text = format!(
                "{}{}{}{}{}{}{}{}",
                self.campaign
                    .string(word(&self.campaign.data, 0x3d7a)? as usize)?,
                self.campaign.province_name(origin as u8)?,
                self.campaign
                    .string(word(&self.campaign.data, 0x3d7e)? as usize)?,
                self.campaign.region_name(target)?,
                self.campaign
                    .string(word(&self.campaign.data, 0x3d82)? as usize)?,
                self.campaign
                    .string(word(&self.campaign.data, 0x1100 + index * 2)? as usize)?,
                self.campaign
                    .string(word(&self.campaign.data, 0x3d88)? as usize)?,
                self.campaign
                    .string(word(&self.campaign.data, 0x3d8e)? as usize)?
            );
            self.phase = Phase::FleetEncounter;
            self.show_legion_message(assets, text, Phase::FleetEncounter)?;
            if let Some(background) = &self.fleet_background {
                self.legion_message_background = Some(background.clone());
            }
            return Ok(());
        }
        if let Some(enemy) = (3..6).find(|i| {
            self.campaign.data[0x1114 + i * 12] == target
                && self.campaign.data[0x1116 + i * 12] != 255
        }) {
            self.start_naval(assets, index, enemy)?;
            return Ok(());
        }
        self.campaign.finish_fleet_sailing(index, target)?;
        self.fleet_destination = None;
        self.fleet_background = None;
        self.cursor = self.campaign.region_coordinate(target)?;
        self.phase = Phase::Map;
        self.moving_fleet = None;
        Ok(())
    }

    fn start_naval(&mut self, assets: &Assets, player: usize, enemy: usize) -> Result<()> {
        let background = self
            .fleet_background
            .clone()
            .ok_or("Missing fleet background")?;
        self.naval = Some(crate::naval::Naval::new(
            assets,
            &self.campaign,
            player,
            enemy,
            background,
        )?);
        self.phase = Phase::Naval;
        self.selected = 0;
        self.highlighted = false;
        Ok(())
    }
    fn fight_naval_army(&mut self, assets: &Assets) -> Result<()> {
        let army = self.fleet_army.ok_or("Missing naval army")?;
        self.campaign.prepare_army_fleet(army, &mut self.rng)?;
        self.start_naval(assets, self.moving_fleet.ok_or("Missing Roman fleet")?, 6)
    }

    fn finish_naval(&mut self, assets: &Assets) -> Result<()> {
        self.campaign.sound.stop();
        let scene = self.naval.take().ok_or("Missing naval result")?;
        if self.enemies.is_some() {
            return self.finish_enemy_naval(assets, scene.result);
        }
        let won = scene.result & 2 != 0 && scene.result & 1 == 0;
        self.fleet_army = None;
        self.selected = 0;
        self.highlighted = false;
        if won {
            self.phase = Phase::FleetMove;
            let target = self
                .fleet_destination
                .ok_or("Missing sea encounter destination")?;
            self.advance_fleet_destination(assets, target)?;
        } else {
            self.phase = Phase::Map;
            self.moving_fleet = None;
            self.fleet_destination = None;
            self.fleet_background = None;
        }
        Ok(())
    }

    fn command_menu(&self) -> usize {
        if self.province == 0 {
            crate::province::ROME_MENU
        } else {
            crate::province::PROVINCE_MENU
        }
    }

    fn move_selection(&mut self, down: bool) -> Result<()> {
        let rows = self.rows();
        let menu = self
            .province_menu()
            .map(|address| crate::province::Menu::read(&self.campaign, address))
            .transpose()?;
        for _ in 0..rows {
            self.selected = (self.selected + if down { 1 } else { rows - 1 }) % rows;
            if menu
                .as_ref()
                .is_none_or(|menu| !menu.items[self.selected].disabled)
            {
                break;
            }
        }
        Ok(())
    }

    fn refresh_province_menu(&mut self) {
        if self.province != 0 {
            let coast = self.campaign.data[0x50ec + usize::from(self.province) * 8];
            self.campaign.data[0x576f] = if (27..255).contains(&coast) { 0 } else { 255 };
        }
    }

    fn render_province(
        &self,
        assets: &Assets,
        menu: usize,
        selected: Option<usize>,
    ) -> Result<Frame> {
        let mut frame = self.province_panels.screen(
            assets,
            &self.campaign,
            self.province_header
                .as_ref()
                .map(|header| (self.province, header)),
            menu,
            selected,
        )?;
        if self.tribute_indicator {
            self.province_panels.tribute_indicator(
                assets,
                &self.campaign,
                &mut frame,
                menu,
                self.province,
                selected,
            )?;
        }
        Ok(frame)
    }

    fn close_province_modal(&mut self) {
        self.phase = Phase::Rome;
        self.refresh_province_menu();
        self.selected = self.province_selection.unwrap_or(0);
        self.highlighted = self.province_selection.is_some()
            && (self.province == 0 || self.province_selection == Some(5));
    }

    fn advance(&mut self) {
        if self.phase == Phase::LegionList {
            self.phase = Phase::Legions;
            self.selected = 4;
            self.highlighted = true;
            return;
        }
        if self.phase == Phase::ProvinceList {
            if (self.list_page + 1) * 14 < self.campaign.owned_provinces().len() {
                self.list_page += 1;
            } else {
                self.close_province_modal();
            }
            return;
        }
        if self.phase == Phase::Notice && self.notice_return == Phase::Rome {
            self.close_province_modal();
            return;
        }
        self.phase = match self.phase {
            Phase::Intro | Phase::Title | Phase::Credits => Phase::Prologue,
            Phase::Prologue | Phase::About | Phase::Notice => Phase::Map,
            other => other,
        };
        self.phase_ticks = 0;
        if self.phase == Phase::Map {
            self.intro_sequence.water.reset();
        }
    }

    fn advance_time(&mut self, count: u32) {
        self.ticks = self.ticks.wrapping_add(count);
        let mut remaining = count;
        while remaining != 0 {
            let Some((duration, next)) = self.intro_sequence.next(self.phase) else {
                self.phase_ticks = self.phase_ticks.wrapping_add(remaining);
                break;
            };
            let step = remaining.min(duration - self.phase_ticks);
            if crate::water::Water::enabled(self.phase) {
                self.intro_sequence.water.advance(step);
            }
            self.phase_ticks += step;
            remaining -= step;
            if self.phase_ticks == duration {
                self.phase = next;
                self.phase_ticks = 0;
                if next == Phase::Map {
                    self.intro_sequence.water.reset();
                }
            }
        }
    }

    fn slot_path(&self, slot: usize) -> Result<PathBuf> {
        self.save_dir
            .as_ref()
            .map(|root| root.join(format!("{slot}.CSV")))
            .ok_or_else(|| "Select a writable save directory with --state-dir.".into())
    }

    fn load_slot(&self, assets: &Assets, slot: usize) -> Result<Vec<u8>> {
        if let Ok(path) = self.slot_path(slot)
            && path.exists()
        {
            return read(&path);
        }
        #[cfg(feature = "dev-tools")]
        if !self.slot_fallback {
            return Err(self
                .slot_errors
                .get(&slot)
                .cloned()
                .unwrap_or_else(|| "Empty save slot".into()));
        }
        read(&assets.original().join(format!("{slot}.CSV")))
    }

    fn activate(&mut self, assets: &Assets) -> Result<()> {
        if let Some(address) = self.province_menu()
            && crate::province::Menu::read(&self.campaign, address)?
                .items
                .get(self.selected)
                .is_some_and(|item| item.disabled)
        {
            return Ok(());
        }
        match self.phase {
            Phase::BattlePreparation => {
                let scene = self.battle.as_mut().ok_or("Missing battle")?;
                match scene.stage {
                    crate::battle::Stage::Formation => {
                        scene.select_formation(&mut self.rng, self.selected)?;
                        self.selected = 0;
                        self.highlighted = false;
                    }
                    crate::battle::Stage::Tactics => {
                        scene.select_tactic(self.selected)?;
                        self.battle_lead_in = if self.skip_battle_fanfare { 0 } else { 432 };
                        self.campaign
                            .sound
                            .play(scene.battle_music() - u8::from(self.battle_lead_in != 0));
                        self.cursor = (159, 99);
                        self.highlighted = false;
                    }
                    crate::battle::Stage::Deployment => {}
                }
            }
            Phase::Diplomacy => {
                self.highlighted = true;
                let scene = self.diplomacy.as_mut().ok_or("Missing conversation")?;
                if !scene.choose(assets, &self.campaign, self.selected)? {
                    let (legion, province) = (scene.legion, scene.province);
                    let background = self.render(assets, false)?;
                    self.battle = Some(crate::battle::Preparation::new(
                        assets,
                        &mut self.campaign,
                        &mut self.rng,
                        legion,
                        province,
                        &background,
                    )?);
                    self.phase = Phase::BattlePreparation;
                    self.selected = 0;
                    self.highlighted = false;
                }
            }
            Phase::Legions => {
                self.legion_menu_row = self.selected;
                match self.selected {
                    0 => {
                        if let Some(address) = self.campaign.recruitment_gate()? {
                            self.show_legion_message(
                                assets,
                                self.campaign.string(address)?,
                                Phase::Legions,
                            )?;
                        } else {
                            self.phase = Phase::RaiseLegion;
                            self.selected = 0;
                            self.highlighted = false;
                        }
                    }
                    1..=3 => {
                        self.legion_operation = self.selected;
                        let local = self.campaign.local_legions(self.legion_province);
                        if local.len() == 1 {
                            self.perform_legion_action(assets, local[0])?;
                        } else if local.len() > 1
                            && self.legion_operation != 3
                            && local
                                .iter()
                                .all(|i| self.campaign.data[0x1926 + i * 20] != 0)
                        {
                            self.show_readiness_message(
                                assets,
                                self.campaign.string(0x1d32)?,
                                Phase::Legions,
                            )?;
                        } else if local.len() > 1 {
                            self.phase = Phase::LegionSelect;
                            self.selected = 0;
                            self.highlighted = true;
                        }
                    }
                    4 => {
                        self.phase = Phase::LegionList;
                    }
                    5 => self.close_province_modal(),
                    _ => unreachable!(),
                }
            }
            Phase::RaiseLegion => {
                if self.selected == 3 {
                    self.phase = Phase::Legions;
                    self.selected = 0;
                    self.highlighted = true;
                } else {
                    let background = self.render(assets, false)?;
                    let outcome = self.campaign.begin_recruitment(
                        self.legion_province,
                        self.selected as u8,
                        &mut self.rng,
                    )?;
                    self.recruitment_payment = outcome.payment;
                    self.legion_message_background = Some(background);
                    self.legion_message = outcome.message;
                    self.legion_message_return = if outcome.success {
                        Phase::Legions
                    } else {
                        Phase::RaiseLegion
                    };
                    self.phase = Phase::LegionMessage;
                }
            }
            Phase::LegionSelect => {
                if let Some(index) = self
                    .campaign
                    .local_legions(self.legion_province)
                    .get(self.selected)
                    .copied()
                {
                    self.perform_legion_action(assets, index)?;
                }
            }
            Phase::Naval => {
                let scene = self.naval.as_mut().ok_or("Missing naval encounter")?;
                if scene.stage == crate::naval::Stage::Ship {
                    scene.select(scene.choices[self.selected])?;
                }
            }
            Phase::FleetSelect => {
                if let Some(&index) = self
                    .campaign
                    .local_fleets(self.fleet_region)
                    .get(self.selected)
                {
                    if self.campaign.data[0x1116 + index * 12] != 0 {
                        self.show_legion_message(
                            assets,
                            self.campaign.string(0x1348)?,
                            Phase::FleetSelect,
                        )?;
                    } else {
                        self.moving_fleet = Some(index);
                        self.phase = Phase::FleetMove;
                    }
                }
            }
            Phase::BoardConfirmation => {
                let boarding = self.boarding.take().ok_or("Missing boarding")?;
                self.legion_message_background = None;
                if self.selected == 0 {
                    boarding.apply(&mut self.campaign)?;
                    self.finish_march(Some(boarding.destination))?;
                } else {
                    self.phase = Phase::LegionMove;
                    self.highlighted = false;
                }
            }
            Phase::FleetConstruction => {
                let scene = self.construction.as_mut().ok_or("Missing construction")?;
                if self.selected < 3 {
                    scene.select(&self.campaign, self.selected)?;
                } else {
                    scene.finish(&mut self.campaign)?;
                    self.construction = None;
                    self.close_province_modal();
                    self.highlighted = false;
                }
            }
            Phase::PlunderConfirmation => {
                if self.selected == 0 {
                    let amount = self.campaign.plunder(self.province)?;
                    let message = if amount == 0 {
                        self.campaign.string(0x5834)?
                    } else {
                        format!(
                            "{}{}{}",
                            self.campaign.string(0x57f4)?,
                            amount,
                            self.campaign.string(0x5812)?
                        )
                    };
                    // The original leaves the old status pane behind the message.
                    let background = self
                        .legion_message_background
                        .take()
                        .ok_or("Missing plunder background")?;
                    self.legion_message_background = Some(background);
                    self.legion_message = message;
                    self.legion_message_return = Phase::Rome;
                    self.phase = Phase::LegionMessage;
                } else {
                    self.province_selection = Some(0);
                    self.close_province_modal();
                }
            }
            Phase::Rome => {
                self.province_selection = Some(self.selected);
                let action = if self.province == 0 {
                    self.selected
                } else {
                    [0, 7, 1, 2, 3, 5, 6][self.selected]
                };
                if action == 2 {
                    let mut background = self.render(assets, false)?;
                    self.province_panels.deactivate_menu(
                        assets,
                        &self.campaign,
                        &mut background,
                        self.command_menu(),
                    )?;
                    self.games = Some(crate::games::Games::new(
                        &self.campaign,
                        self.province,
                        background,
                    )?);
                    self.phase = Phase::Games;
                    return Ok(());
                }
                if action == 7 {
                    let mut background = self.render(assets, false)?;
                    if self.campaign.local_legions(self.province).is_empty() {
                        self.legion_message_background = Some(background);
                        self.legion_message = self.campaign.string(0x57cc)?;
                        self.legion_message_return = Phase::Rome;
                        self.phase = Phase::LegionMessage;
                    } else {
                        self.province_panels.deactivate_menu(
                            assets,
                            &self.campaign,
                            &mut background,
                            self.command_menu(),
                        )?;
                        self.legion_message_background = Some(background);
                        self.phase = Phase::PlunderConfirmation;
                        self.selected = 1;
                        self.highlighted = false;
                    }
                    return Ok(());
                }
                self.phase = match action {
                    0 => {
                        self.legion_province = self.province;
                        self.legion_selection_on_map = false;
                        self.selected = 0;
                        self.highlighted = false;
                        Phase::Legions
                    }
                    1 => {
                        self.selected = self.campaign.tribute(self.province)? as usize;
                        self.highlighted = false;
                        Phase::Tribute
                    }
                    3 => match self.campaign.shipbuilding_site(self.province)? {
                        crate::fleet::BuildSite::Unavailable(address) => {
                            self.show_legion_message(
                                assets,
                                self.campaign.string(address)?,
                                Phase::Rome,
                            )?;
                            return Ok(());
                        }
                        crate::fleet::BuildSite::Available(slot) => {
                            self.construction = Some(crate::fleet::Construction::new(
                                &self.campaign,
                                self.province,
                                slot,
                                self.render(assets, false)?,
                            )?);
                            self.selected = 0;
                            self.highlighted = false;
                            Phase::FleetConstruction
                        }
                    },
                    4 => {
                        self.selected = self.campaign.data[0x5fc0] as usize;
                        self.highlighted = false;
                        Phase::TributePolicy
                    }
                    5 => {
                        self.list_page = 0;
                        Phase::ProvinceList
                    }
                    6 => Phase::Map,
                    _ => {
                        let menu =
                            crate::province::Menu::read(&self.campaign, self.command_menu())?;
                        self.notice =
                            format!("{}\nNot implemented yet.", menu.items[self.selected].label);
                        self.notice_return = Phase::Rome;
                        Phase::Notice
                    }
                };
            }
            Phase::Tribute | Phase::TributePolicy => {
                if self.selected < 5 {
                    if self.phase == Phase::Tribute {
                        self.campaign
                            .set_tribute(self.province, self.selected as u8)?;
                    } else {
                        self.campaign.set_tribute_policy(self.selected as u8)?;
                    }
                }
                self.close_province_modal();
            }
            Phase::Menu => {
                self.phase = match self.selected {
                    0 => Phase::SaveSlots,
                    1 => Phase::LoadSlots,
                    2 => {
                        self.campaign = Campaign::new(assets)?;
                        self.selected_province = None;
                        Phase::Map
                    }
                    3 => Phase::About,
                    4 => Phase::Difficulty,
                    5 => {
                        self.quit = true;
                        Phase::Map
                    }
                    6 => Phase::Map,
                    _ => return Err("Invalid menu selection".into()),
                };
                self.selected = 0;
            }
            Phase::Difficulty => {
                self.campaign.data[0x48e..0x492].fill(self.selected as u8);
                self.phase = Phase::Map;
            }
            Phase::SaveSlots => {
                self.slot = self.selected;
                self.description = self.campaign.string(0xa8f0)?;
                self.phase = Phase::SaveDescription;
            }
            Phase::LoadSlots => {
                match self
                    .load_slot(assets, self.selected)
                    .and_then(|save| self.campaign.load(&save))
                {
                    Ok(()) => {
                        self.phase = Phase::Map;
                        self.selected_province = None;
                    }
                    Err(error) => self.show_error(error),
                }
            }
            Phase::SaveDescription => {
                let save_result = (|| {
                    let path = self.slot_path(self.slot)?;
                    let parent = path.parent().ok_or("Invalid save path")?;
                    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                    let mut save = self.campaign.save();
                    save[4..52].fill(0);
                    save[4..4 + self.description.len()]
                        .copy_from_slice(self.description.as_bytes());
                    std::fs::write(&path, &save).map_err(|e| format!("{}: {e}", path.display()))?;
                    self.campaign.load(&save)?;
                    Ok(())
                })();
                match save_result {
                    Ok(()) => self.phase = Phase::Map,
                    Err(error) => self.show_error(error),
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn show_legion_message(&mut self, assets: &Assets, message: String, next: Phase) -> Result<()> {
        self.legion_message_background = Some(self.render(assets, false)?);
        self.legion_message = message;
        self.legion_message_return = next;
        self.phase = Phase::LegionMessage;
        Ok(())
    }

    fn perform_legion_action(&mut self, assets: &Assets, index: usize) -> Result<()> {
        if self.legion_operation != 3 && self.campaign.data[0x1926 + index * 20] != 0 {
            let next = if self.phase == Phase::LegionSelect {
                Phase::LegionSelect
            } else {
                Phase::Legions
            };
            self.show_readiness_message(assets, self.campaign.string(0x1d0e)?, next)?;
            return Ok(());
        }
        if self.legion_operation == 2 {
            let region = self.campaign.legion_location(index)?;
            if !self.legion_selection_on_map {
                self.cursor = self.campaign.region_coordinate(region)?;
            }
            self.phase = Phase::LegionMove;
            self.moving_legion = Some(index);
            return Ok(());
        }
        let mut background = self.render(assets, false)?;
        self.province_panels.deactivate_menu(
            assets,
            &self.campaign,
            &mut background,
            crate::legion::MENU,
        )?;
        if self.phase == Phase::LegionSelect {
            let count = self
                .campaign
                .local_legions(self.legion_province)
                .len()
                .min(3);
            background.rect(8, 24, 9, count as i32 * 10, 26);
        }
        let message = if self.legion_operation == 1 {
            self.campaign
                .strengthen_legion(self.legion_province, index)?
        } else {
            Some(self.campaign.upgrade_legion(index)?)
        };
        if let Some(message) = message {
            self.legion_message_background = Some(background);
            self.legion_message = message;
            self.legion_message_return = Phase::Legions;
            self.phase = Phase::LegionMessage;
        } else {
            self.phase = Phase::Legions;
            self.selected = self.legion_menu_row;
            self.highlighted = true;
        }
        Ok(())
    }

    fn start_map_march(&mut self, assets: &Assets) -> Result<()> {
        let Some(region) = self.campaign.region_at(self.cursor.0, self.cursor.1)? else {
            return Ok(());
        };
        let local = self.campaign.local_legions(region);
        if local.is_empty() {
            return Ok(());
        }
        self.legion_province = region;
        self.legion_selection_on_map = true;
        self.legion_operation = 2;
        self.selected = 0;
        if local.len() == 1 && self.campaign.data[0x1926 + local[0] * 20] == 0 {
            self.perform_legion_action(assets, local[0])?;
        } else {
            self.phase = Phase::LegionSelect;
            self.highlighted = true;
            if local
                .iter()
                .all(|i| self.campaign.data[0x1926 + i * 20] != 0)
            {
                let message = if local.len() == 1 { 0x1d0e } else { 0x1d32 };
                self.show_legion_message(assets, self.campaign.string(message)?, Phase::Map)?;
            }
        }
        Ok(())
    }

    fn show_readiness_message(
        &mut self,
        assets: &Assets,
        message: String,
        next: Phase,
    ) -> Result<()> {
        let deactivate = self.phase == Phase::Legions;
        self.show_legion_message(assets, message, next)?;
        if deactivate {
            self.province_panels.deactivate_menu(
                assets,
                &self.campaign,
                self.legion_message_background.as_mut().unwrap(),
                crate::legion::MENU,
            )?;
        }
        Ok(())
    }

    fn choose_march_destination(&mut self, assets: &Assets) -> Result<()> {
        let Some(index) = self.moving_legion else {
            return Err("No selected legion for movement".into());
        };
        let Some(region) = self.campaign.region_at(self.cursor.0, self.cursor.1)? else {
            return Ok(());
        };
        match self.campaign.march_legion(index, region)? {
            crate::movement::March::Ignored => {}
            crate::movement::March::Moved => {
                self.finish_march(Some(region))?;
            }
            crate::movement::March::Cancelled => {
                self.finish_march(None)?;
            }
            crate::movement::March::EncounterPending => {
                let background = self.render(assets, false)?;
                let battlefield_background =
                    crate::campaign::render(assets, &self.campaign, &self.map, &self.sprites)?;
                self.encounter = Some(crate::encounter::Encounter::new(
                    index,
                    region,
                    background,
                    battlefield_background,
                ));
                self.campaign.data[0x1926 + index * 20] = 1;
                self.advance_encounter(assets)?;
            }
            crate::movement::March::EmbarkPending => {
                let boarding = crate::fleet::Boarding::prepare(&self.campaign, index, region)?;
                if boarding.left != 0 {
                    self.legion_message_background = Some(self.render(assets, false)?);
                    self.boarding = Some(boarding);
                    self.phase = Phase::BoardConfirmation;
                    self.selected = 1;
                    self.highlighted = false;
                } else {
                    boarding.apply(&mut self.campaign)?;
                    self.finish_march(Some(region))?;
                }
            }
        }
        Ok(())
    }

    fn advance_encounter(&mut self, assets: &Assets) -> Result<()> {
        use crate::encounter::Action;
        let march = self.encounter.as_mut().ok_or("Missing march encounter")?;
        let action = march.next(&self.campaign)?;
        let (legion, province) = (march.legion, march.province);
        match action {
            Action::Warn(army) => {
                let text = march.notice(&self.campaign, army)?;
                self.phase = Phase::Encounter;
                self.show_legion_message(assets, text, Phase::Encounter)?;
            }
            Action::Fight(army) => {
                self.battle = Some(crate::battle::Preparation::against(
                    assets,
                    &mut self.campaign,
                    &mut self.rng,
                    legion,
                    province,
                    army,
                    &march.battlefield_background,
                )?);
                self.phase = Phase::BattlePreparation;
                self.selected = 0;
                self.highlighted = false;
            }
            Action::Friendly => {
                self.campaign.complete_friendly_march(legion, province)?;
                self.encounter = None;
                self.finish_march(Some(province))?;
            }
            Action::Defeated => {
                self.encounter = None;
                self.finish_march(None)?;
            }
            Action::Negotiate => {
                self.encounter = None;
                if crate::diplomacy::Diplomacy::available(&self.campaign, province) {
                    self.diplomacy = Some(crate::diplomacy::Diplomacy::new(
                        assets,
                        &mut self.campaign,
                        &mut self.rng,
                        legion,
                        province,
                    )?);
                    self.phase = Phase::Diplomacy;
                    self.selected = 0;
                    self.highlighted = false;
                } else {
                    self.finish_march(None)?;
                    self.show_legion_message(assets, "The roaming-army encounters have been resolved.\nThis province's special scene is not implemented yet.".into(), Phase::Map)?;
                }
            }
        }
        Ok(())
    }

    fn advance_turn(&mut self, assets: &Assets) -> Result<()> {
        loop {
            let stage = self.turn_stage.ok_or("Missing turn continuation")?;
            self.turn_stage = Some(stage + 1);
            match stage {
                province @ 0..=26 => {
                    if self.campaign.turn_province_economy(province)? {
                        self.revolt_background = Some(crate::campaign::render(
                            assets,
                            &self.campaign,
                            &self.map,
                            &self.sprites,
                        )?);
                        self.revolt =
                            Some(crate::turn::Revolt::begin(&mut self.campaign, province)?);
                        self.phase = Phase::TurnRevolt;
                        return self.advance_revolt(assets);
                    }
                    self.turn_income = self
                        .turn_income
                        .wrapping_add(self.campaign.turn_province_finish(province)?);
                }
                27 => {
                    self.enemies = Some(crate::enemy::EnemyTurn::new(&self.campaign)?);
                    self.phase = Phase::TurnEnemy;
                    return self.advance_enemies(assets);
                }
                28 => self.campaign.turn_finish_year(self.turn_income)?,
                29 | 30 if word(&self.campaign.data, 0x634c)? > 4 => {
                    let notice = if stage == 29 {
                        crate::events::unrest(&mut self.campaign, &mut self.rng)?
                    } else {
                        crate::events::random_event(
                            &mut self.campaign,
                            &mut self.rng,
                            self.turn_income,
                        )?
                    };
                    if let Some(text) = notice {
                        self.phase = Phase::Map;
                        return self.show_legion_message(assets, text, Phase::TurnContinue);
                    }
                }
                29 | 30 => {}
                _ => {
                    self.turn_stage = None;
                    self.phase = Phase::Map;
                    self.highlighted = false;
                    if !(0..27).any(|p| self.campaign.data[0x5285 + p * 32] != 0) {
                        self.show_legion_message(
                            assets,
                            self.campaign.string(0x17f)?,
                            Phase::Title,
                        )?;
                    }
                    if self.campaign.data[0x5971] == 8 {
                        self.show_legion_message(
                            assets,
                            crate::ending::statistics(&self.campaign)?,
                            Phase::VictoryEpilogue,
                        )?;
                    }
                    return Ok(());
                }
            }
        }
    }

    fn advance_revolt(&mut self, assets: &Assets) -> Result<()> {
        use crate::turn::RevoltAction as R;
        let scene = self.revolt.as_mut().ok_or("Missing revolt")?;
        let action = scene.next(&mut self.campaign)?;
        let province = scene.province;
        match action {
            R::Warn(legion) => {
                let text = scene.warning(&self.campaign, legion)?;
                self.show_legion_message(assets, text, Phase::TurnRevolt)?;
                if let Some(background) = &self.revolt_background {
                    self.legion_message_background = Some(background.clone());
                }
            }
            R::Fight(legion) => {
                let background =
                    crate::campaign::render(assets, &self.campaign, &self.map, &self.sprites)?;
                self.battle = Some(crate::battle::Preparation::against(
                    assets,
                    &mut self.campaign,
                    &mut self.rng,
                    legion,
                    province,
                    None,
                    &background,
                )?);
                self.phase = Phase::BattlePreparation;
                self.selected = 0;
                self.highlighted = false;
            }
            R::Report => {
                let text = scene.report(&self.campaign)?;
                self.show_legion_message(assets, text, Phase::TurnRevolt)?;
                if let Some(background) = &self.revolt_background {
                    self.legion_message_background = Some(background.clone());
                }
            }
            R::Done => {
                self.turn_income = self
                    .turn_income
                    .wrapping_add(self.campaign.turn_province_finish(province)?);
                self.revolt = None;
                self.revolt_background = None;
                self.phase = Phase::Map;
                self.highlighted = false;
                if self.turn_stage.is_some() {
                    return self.advance_turn(assets);
                }
            }
        }
        Ok(())
    }

    fn advance_enemies(&mut self, assets: &Assets) -> Result<()> {
        use crate::enemy::EnemyAction as A;
        if self.enemy_background.is_none() {
            self.enemy_background = Some(crate::campaign::render(
                assets,
                &self.campaign,
                &self.map,
                &self.sprites,
            )?);
        }
        let action = self
            .enemies
            .as_mut()
            .ok_or("Missing enemy turn")?
            .next(&mut self.campaign, &mut self.rng)?;
        match action {
            A::Notice(text) => {
                let background = match &self.enemy_background {
                    Some(frame) => frame.clone(),
                    None => {
                        crate::campaign::render(assets, &self.campaign, &self.map, &self.sprites)?
                    }
                };
                self.show_legion_message(assets, text, Phase::TurnEnemy)?;
                self.legion_message_background = Some(background);
            }
            A::Land {
                army,
                legion,
                province,
            } => {
                let background =
                    crate::campaign::render(assets, &self.campaign, &self.map, &self.sprites)?;
                self.battle = Some(crate::battle::Preparation::against(
                    assets,
                    &mut self.campaign,
                    &mut self.rng,
                    legion,
                    province,
                    Some(army),
                    &background,
                )?);
                self.cursor = (159, 99);
                self.phase = Phase::BattlePreparation;
                self.selected = 0;
                self.highlighted = false;
            }
            A::Naval { army: _, fleet } => {
                self.fleet_background = Some(crate::campaign::render(
                    assets,
                    &self.campaign,
                    &self.map,
                    &self.sprites,
                )?);
                self.start_naval(assets, fleet, 6)?;
            }
            A::Done => {
                self.enemies = None;
                self.enemy_background = None;
                self.fleet_background = None;
                self.phase = Phase::Map;
                if self.turn_stage.is_some() {
                    return self.advance_turn(assets);
                }
            }
        }
        Ok(())
    }
    fn finish_enemy_naval(&mut self, assets: &Assets, result: u16) -> Result<()> {
        let notice = self
            .enemies
            .as_mut()
            .ok_or("Missing defensive naval turn")?
            .naval_returned(&mut self.campaign, result)?;
        self.fleet_background = None;
        self.phase = Phase::TurnEnemy;
        if let Some(text) = notice {
            self.show_legion_message(assets, text, Phase::TurnEnemy)
        } else {
            self.advance_enemies(assets)
        }
    }
    fn complete_land_battle(&mut self, assets: &Assets, victory: bool) -> Result<()> {
        self.battle_lead_in = 0;
        self.campaign.sound.stop();
        if self.enemies.is_some() {
            self.enemy_background = Some(self.render(assets, false)?);
            let enemies = self.enemies.as_mut().unwrap();
            let legion = self
                .battle
                .as_ref()
                .ok_or("Missing defensive battle")?
                .legion;
            enemies.land_returned(&mut self.campaign, legion, victory)?;
            self.battle = None;
            self.phase = Phase::TurnEnemy;
            return self.advance_enemies(assets);
        }

        if let Some(revolt) = self.revolt.as_mut() {
            let legion = self.battle.as_ref().ok_or("Missing revolt battle")?.legion;
            revolt.battle_returned(&mut self.campaign, legion, victory)?;
            self.revolt_background = None;
            self.battle = None;
            self.phase = Phase::TurnRevolt;
            return self.advance_revolt(assets);
        }

        let scene = self.battle.as_ref().ok_or("Missing battle")?;
        let province = scene.province;
        if let Some(army) = scene.army {
            let background = if scene.outcome.is_some() {
                scene.render(assets, &self.campaign, None)?
            } else {
                scene.field_frame()?
            };
            let march = self.encounter.as_mut().ok_or("Missing intercepted march")?;
            march.battlefield_background = background.clone();
            march.background = background;
            march.battle_returned(&mut self.campaign, army, victory);
            self.battle = None;
            self.advance_encounter(assets)?;
        } else {
            let legion = self.moving_legion.ok_or("Missing battle legion")?;
            if victory {
                let base = usize::from(province) * 32;
                self.campaign.data[0x5285 + base] = 1;
                self.campaign.data[0x5289 + base] = 0;
                self.campaign.data[0x5282 + base] = self.campaign.data[0x5fc0];
                self.campaign.finish_foreign_march(legion, province, 1)?;
            }
            self.battle = None;
            self.encounter = None;
            self.diplomacy = None;
            self.finish_march(victory.then_some(province))?;
        }
        Ok(())
    }

    fn advance_diplomacy(&mut self, assets: &Assets) -> Result<()> {
        let scene = self.diplomacy.as_ref().ok_or("Missing conversation")?;
        if scene.completed() {
            let target = if scene.allied() {
                Some(scene.province)
            } else {
                None
            };
            if let Some(province) = target {
                self.campaign
                    .finish_alliance_march(scene.legion, province)?;
            }
            self.diplomacy = None;
            self.finish_march(target)?;
        } else {
            self.diplomacy
                .as_mut()
                .unwrap()
                .advance(assets, &mut self.campaign, &mut self.rng)?;
            if self.diplomacy.as_ref().unwrap().stage == crate::diplomacy::Stage::Complete {
                return self.advance_diplomacy(assets);
            }
            if self.diplomacy.as_ref().unwrap().stage == crate::diplomacy::Stage::Choice {
                self.selected = 0;
                self.highlighted = false;
            }
        }
        Ok(())
    }

    fn finish_march(&mut self, destination: Option<u8>) -> Result<()> {
        // Closing the province command loop redraws its original location;
        // a march started directly on the map focuses the destination instead.
        let focus = if self.legion_selection_on_map {
            destination
        } else {
            Some(self.legion_province)
        };
        if let Some(region) = focus {
            self.cursor = self.campaign.region_coordinate(region)?;
        }
        self.phase = Phase::Map;
        self.moving_legion = None;
        Ok(())
    }

    fn show_error(&mut self, message: String) {
        eprintln!("centurion-prototype: {message}");
        self.notice = message;
        self.notice_return = Phase::Map;
        self.phase = Phase::Notice;
    }

    pub fn export_battle(&self, directory: &Path, step: usize) -> Result<()> {
        if let Some(scene) = &self.naval {
            std::fs::create_dir_all(directory).map_err(|e| e.to_string())?;
            std::fs::write(
                directory.join(format!("naval-{step:03}.ds")),
                &scene.view.data,
            )
            .map_err(|e| e.to_string())?;
        }

        if let Some(scene) = &self.battle {
            let records: Vec<u8> = scene.units.iter().flatten().copied().collect();
            std::fs::write(directory.join(format!("units-{step:03}.bin")), records)
                .map_err(|e| e.to_string())?;
            std::fs::write(directory.join(format!("grid-{step:03}.bin")), scene.grid())
                .map_err(|e| e.to_string())?;
        }
        if let Some(frame) = self.games.as_ref().and_then(|g| g.race_field_frame()) {
            std::fs::write(
                directory.join(format!("race-field-{step:03}.idx")),
                &frame.pixels,
            )
            .map_err(|e| e.to_string())?;
        }
        if self.games.as_ref().is_some_and(|g| g.has_race()) {
            std::fs::write(
                directory.join(format!("race-{step:03}.ds")),
                &self.campaign.data,
            )
            .map_err(|e| e.to_string())?;
        }
        std::fs::write(
            directory.join(format!("campaign-{step:03}.CSV")),
            self.campaign.save(),
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn render(&self, assets: &Assets, cursor: bool) -> Result<Frame> {
        let mut frame = Frame::new();
        match self.phase {
            Phase::Intro => frame.blit(&self.intro, 0, 0),
            Phase::Title => {
                frame.pixels.copy_from_slice(&self.world);
                frame.blit(&self.title, 0, 64);
            }
            Phase::Credits | Phase::Prologue => frame.pixels.copy_from_slice(&self.world),
            Phase::Rome | Phase::Tribute | Phase::TributePolicy | Phase::ProvinceList => {
                let menu = self.province_menu().unwrap_or(self.command_menu());
                let selected = if self.phase == Phase::ProvinceList {
                    self.province_selection
                } else {
                    self.highlighted.then_some(self.selected)
                };
                frame = self.render_province(assets, menu, selected)?;
                if self.phase == Phase::ProvinceList {
                    self.province_panels.list(
                        assets,
                        &self.campaign,
                        &mut frame,
                        self.list_page,
                    )?;
                }
            }
            Phase::LegionSelect if self.legion_selection_on_map => {
                frame = crate::campaign::render(assets, &self.campaign, &self.map, &self.sprites)?;
                let ready = self
                    .campaign
                    .local_legions(self.legion_province)
                    .iter()
                    .any(|i| self.campaign.data[0x1926 + i * 20] == 0);
                crate::legion::local_list(
                    &mut frame,
                    assets,
                    &self.campaign,
                    ready.then_some(self.selected),
                    self.legion_province,
                )?;
            }
            Phase::Legions | Phase::RaiseLegion | Phase::LegionList | Phase::LegionSelect => {
                let menu = if self.phase == Phase::RaiseLegion {
                    crate::legion::RAISE_MENU
                } else {
                    crate::legion::MENU
                };
                let selected =
                    if self.phase == Phase::LegionList || self.phase == Phase::LegionSelect {
                        Some(self.legion_menu_row)
                    } else {
                        self.highlighted.then_some(self.selected)
                    };
                frame = self.render_province(assets, menu, selected)?;
                if self.phase == Phase::LegionSelect {
                    self.province_panels.deactivate_menu(
                        assets,
                        &self.campaign,
                        &mut frame,
                        crate::legion::MENU,
                    )?;
                }
                if self.phase == Phase::LegionList {
                    crate::legion::complete_list(&mut frame, assets, &self.campaign)?;
                } else {
                    crate::legion::local_list(
                        &mut frame,
                        assets,
                        &self.campaign,
                        (self.phase == Phase::LegionSelect).then_some(self.selected),
                        self.legion_province,
                    )?;
                }
            }
            Phase::BattlePreparation => {
                frame = self.battle.as_ref().ok_or("Missing battle")?.render(
                    assets,
                    &self.campaign,
                    self.highlighted.then_some(self.selected),
                )?;
            }
            Phase::Encounter => {
                frame = self
                    .encounter
                    .as_ref()
                    .ok_or("Missing encounter background")?
                    .background
                    .clone()
            }
            Phase::Diplomacy => {
                frame = self
                    .diplomacy
                    .as_ref()
                    .ok_or("Missing conversation")?
                    .render(
                        assets,
                        &self.campaign,
                        self.highlighted.then_some(self.selected),
                    )?;
            }
            Phase::LegionMove => {
                frame = crate::campaign::render(assets, &self.campaign, &self.map, &self.sprites)?;
                crate::movement::render(
                    assets,
                    &self.campaign,
                    &mut frame,
                    &self.sprites,
                    self.moving_legion.ok_or("No selected moving legion")?,
                )?;
            }
            Phase::Games => {
                frame = self.games.as_ref().ok_or("Missing games scene")?.render(
                    assets,
                    &self.campaign,
                    &self.province_panels,
                )?;
            }
            Phase::VictoryParade => {
                frame = self
                    .victory_parade
                    .as_ref()
                    .ok_or("Missing victory parade")?
                    .render();
            }
            Phase::VictoryCredits => {
                frame = self
                    .victory_credits
                    .as_ref()
                    .ok_or("Missing ending credits")?
                    .render(assets)?;
            }
            Phase::Naval => {
                frame = self
                    .naval
                    .as_ref()
                    .ok_or("Missing naval encounter")?
                    .render(assets, self.highlighted.then_some(self.selected))?;
                if self.naval.as_ref().unwrap().stage == crate::naval::Stage::Reengage {
                    self.province_panels.confirm(
                        assets,
                        &self.campaign,
                        &mut frame,
                        &self.campaign.string(0x935a)?,
                        Some(self.selected),
                    )?;
                }
            }
            Phase::RegionInspect => {
                frame = crate::campaign::render(assets, &self.campaign, &self.map, &self.sprites)?;
                crate::fleet_nav::region_inspection(
                    &mut frame,
                    assets,
                    &self.campaign,
                    self.inspected_region.ok_or("Missing inspected region")?,
                    self.fleet_information,
                )?;
            }
            Phase::FleetMove | Phase::FleetSelect | Phase::FleetList | Phase::FleetInspect => {
                frame = crate::campaign::render(assets, &self.campaign, &self.map, &self.sprites)?;
                match self.phase {
                    Phase::FleetMove => crate::fleet_nav::movement(
                        &mut frame,
                        assets,
                        &self.campaign,
                        &self.sprites,
                        self.moving_fleet.ok_or("Missing fleet")?,
                    )?,
                    Phase::FleetList => {
                        crate::fleet_nav::complete_list(&mut frame, assets, &self.campaign)?
                    }
                    Phase::FleetInspect => crate::fleet_nav::inspection(
                        &mut frame,
                        assets,
                        &self.campaign,
                        self.moving_fleet.ok_or("Missing fleet")?,
                        self.fleet_information,
                    )?,
                    _ => {
                        let ids = self.campaign.local_fleets(self.fleet_region);
                        let ready = ids.iter().any(|i| self.campaign.data[0x1116 + i * 12] == 0);
                        let title = (ids.len() > 1 && ready)
                            .then(|| self.campaign.string(0x1390))
                            .transpose()?;
                        crate::fleet_nav::local(
                            &mut frame,
                            assets,
                            &self.campaign,
                            &ids,
                            title.as_deref(),
                            ready.then_some(if self.highlighted {
                                self.selected
                            } else {
                                usize::MAX
                            }),
                        )?;
                    }
                }
            }
            Phase::BoardConfirmation => {
                frame = self
                    .legion_message_background
                    .as_ref()
                    .ok_or("Missing boarding background")?
                    .clone();
                let question = self
                    .boarding
                    .as_ref()
                    .ok_or("Missing boarding")?
                    .question(&self.campaign)?;
                self.province_panels.confirm(
                    assets,
                    &self.campaign,
                    &mut frame,
                    &question,
                    self.highlighted.then_some(self.selected),
                )?;
            }
            Phase::FleetConstruction => {
                frame = self
                    .construction
                    .as_ref()
                    .ok_or("Missing construction")?
                    .render(
                        assets,
                        &self.campaign,
                        &self.province_panels,
                        self.highlighted.then_some(self.selected),
                    )?;
            }
            Phase::PlunderConfirmation => {
                frame = self
                    .legion_message_background
                    .as_ref()
                    .ok_or("Missing confirmation background")?
                    .clone();
                self.province_panels.confirm_plunder(
                    assets,
                    &self.campaign,
                    &mut frame,
                    self.highlighted.then_some(self.selected),
                )?;
            }
            Phase::LegionMessage => {
                frame = self
                    .legion_message_background
                    .as_ref()
                    .ok_or("Missing legion message background")?
                    .clone();
                crate::legion::message_with_footer_offset(
                    &mut frame,
                    assets,
                    &self.campaign,
                    &self.legion_message,
                    if self.campaign.data[0x5971] == 8
                        && matches!(
                            self.legion_message_return,
                            Phase::VictoryEpilogue | Phase::VictoryParade
                        )
                    {
                        -1
                    } else {
                        0
                    },
                )?;
            }
            Phase::Notice if self.notice_return == Phase::Rome => {
                frame =
                    self.render_province(assets, self.command_menu(), self.province_selection)?;
            }
            _ => frame = crate::campaign::render(assets, &self.campaign, &self.map, &self.sprites)?,
        }
        self.intro_sequence
            .render(assets, &mut frame, self.phase, self.phase_ticks)?;
        if self.phase == Phase::Menu {
            self.render_game_menu(assets, &mut frame)?;
        } else if matches!(
            self.phase,
            Phase::SaveSlots | Phase::LoadSlots | Phase::Difficulty
        ) {
            let title = match self.phase {
                Phase::SaveSlots => "Select a Save Position",
                Phase::LoadSlots => "Select a Game to Load",
                _ => "Difficulty Level Menu",
            };
            frame.bevel(30, 40, 290, 166, 26);
            frame.center_text(assets, title, 160, 46, 15)?;
            crate::province::decorate_menu(
                &mut frame,
                &self.sprites[1],
                &self.campaign,
                &(0..self.rows())
                    .map(|row| {
                        let top = 68 + row as i32 * 10;
                        (39, top, 284, top + 10)
                    })
                    .collect::<Vec<_>>(),
                Some(self.selected),
                true,
            );
            for row in 0..self.rows() {
                let label = if self.phase == Phase::Difficulty {
                    ["Galley Slave", "Legionnaire", "Senator", "Emperor"][row].to_string()
                } else {
                    let description = self
                        .load_slot(assets, row)
                        .ok()
                        .and_then(|save| {
                            if save.len() != 1780 || save.get(..4) != Some(b"10A\0") {
                                return None;
                            }
                            let end = save[4..52].iter().position(|b| *b == 0).unwrap_or(48);
                            Some(String::from_utf8_lossy(&save[4..4 + end]).into_owned())
                        })
                        .unwrap_or_else(|| "Empty".into());
                    format!("{}  {description}", row + 1)
                };
                frame.text(
                    assets,
                    &label,
                    50,
                    68 + row as i32 * 10,
                    if row == self.selected { 15 } else { 0 },
                )?;
            }
        } else if self.phase == Phase::SaveDescription {
            frame.bevel(30, 70, 290, 123, 26);
            frame.center_text(
                assets,
                &format!("Save Slot {} - Description", self.slot + 1),
                160,
                77,
                15,
            )?;
            frame.rect(40, 99, 240, 13, 8);
            frame.text(assets, &self.description, 43, 101, 15)?;
        } else if self.phase == Phase::About {
            frame.acknowledgment_popup = true;
            frame.bevel(18, 48, 302, 152, 26);
            frame.text(
                assets,
                &format!(
                    "CENTURION\nDefender of Rome\nVersion 1.00A 5/25/90\n{}",
                    self.campaign.continue_footer()?
                ),
                34,
                64,
                15,
            )?;
        } else if self.phase == Phase::Notice {
            frame.acknowledgment_popup = true;
            frame.bevel(16, 70, 304, 133, 26);
            if self.notice_return == Phase::Rome {
                frame.center_lines(
                    assets,
                    &format!("{}\n{}", self.notice, self.campaign.continue_footer()?),
                    160,
                    81,
                    15,
                )?;
            } else {
                frame.text(
                    assets,
                    &format!(
                        "Cannot complete this operation.\nSee terminal for details.\n{}",
                        self.campaign.continue_footer()?
                    ),
                    26,
                    81,
                    15,
                )?;
            }
        }
        let battle_cursor = self.battle_pointer_visible();
        let show_pointer = if frame.acknowledgment_popup {
            self.popup_cursor == reforge_settings::PopupCursor::Visible
        } else {
            (self.phase != Phase::Naval
                || self.naval.as_ref().is_some_and(|scene| {
                    matches!(
                        scene.stage,
                        crate::naval::Stage::Ship | crate::naval::Stage::Reengage
                    )
                }))
                && self.phase != Phase::VictoryParade
                && self.phase != Phase::VictoryCredits
                && (self.phase != Phase::Games
                    || self.games.as_ref().is_some_and(|s| s.cursor_visible()))
                && !self.phase.is_startup()
                && self.phase != Phase::LegionMessage
                && self.phase != Phase::RegionInspect
                && (self.phase != Phase::Diplomacy
                    || self.diplomacy.as_ref().is_some_and(|s| s.cursor_visible()))
                && battle_cursor
        };
        if cursor && show_pointer {
            frame.blit(&self.sprites[0], self.drawn_cursor.0, self.drawn_cursor.1);
        }
        if frame.palette.is_none() {
            frame.palette = Some(
                self.intro_sequence
                    .water
                    .palette(&assets.palette, self.phase),
            );
        }
        Ok(frame)
    }

    fn render_game_menu(&self, assets: &Assets, frame: &mut Frame) -> Result<()> {
        frame.bevel(81, 79, 251, 197, 26);
        frame.center_text(assets, "Game Control Menu", 166, 85, 15)?;
        crate::province::decorate_menu(
            frame,
            &self.sprites[1],
            &self.campaign,
            &(0..7)
                .map(|row| (87, 102 + row * 10, 250, 112 + row * 10))
                .collect::<Vec<_>>(),
            self.highlighted.then_some(self.selected),
            true,
        );
        for row in 0..7 {
            let record = 0x492 + row * 12;
            let string = crate::assets::word(&self.campaign.data, record)? as usize;
            frame.text(
                assets,
                &self.campaign.string(string)?,
                98,
                102 + row as i32 * 10,
                if self.highlighted && row == self.selected {
                    15
                } else {
                    0
                },
            )?;
        }
        Ok(())
    }

    pub fn state_summary(&self) -> String {
        format!(
            "phase={:?} selection={} ticks={} rng={:04x},{:04x} cursor={},{} state_bytes={} quit={} province={:?} random={} {}",
            self.phase,
            self.selected,
            self.ticks,
            self.rng.first,
            self.rng.second,
            self.cursor.0,
            self.cursor.1,
            self.campaign.save().len(),
            self.quit,
            self.selected_province
                .and_then(|p| self.campaign.province_name(p).ok()),
            self.last_random,
            self.games
                .as_ref()
                .map(|s| s.diagnostic())
                .or_else(|| self.naval.as_ref().map(|s| s.diagnostic()))
                .or_else(|| self.diplomacy.as_ref().map(|s| s.diagnostic()))
                .unwrap_or_default()
        )
    }
}
