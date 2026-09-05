use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::catalog::property_catalog;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Project {
    pub format_version: u32,
    pub name: String,
    pub slug: String,
    pub author: String,
    pub root: PathBuf,
    pub nextbots: Vec<Nextbot>,
}

impl Project {
    pub fn unique_class_name(&self, requested: &str) -> String {
        let base = sanitize_class_name(requested);
        let mut candidate = base.clone();
        let mut suffix = 2;
        while self.nextbots.iter().any(|bot| bot.class_name == candidate) {
            candidate = format!("{base}_{suffix}");
            suffix += 1;
        }
        candidate
    }

    pub fn new(name: impl Into<String>, root: PathBuf) -> Self {
        let name = name.into();
        let slug = slugify(&name);
        Self {
            format_version: 1,
            name,
            slug,
            author: String::new(),
            root,
            nextbots: vec![Nextbot::new("My Nextbot", "npc_my_nextbot")],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Nextbot {
    pub display_name: String,
    pub class_name: String,
    pub description: String,
    pub spawn_tab: SpawnTab,
    pub custom_tab_name: String,
    pub category: String,
    pub admin_only: bool,
    #[serde(default = "default_true")]
    pub ignore_nextbots: bool,
    pub base: BaseVariant,
    pub properties: BTreeMap<String, PropertyValue>,
    pub visual: VisualSettings,
    pub audio: AudioSettings,
    pub combat: CombatSettings,
    pub possession_views: Vec<PossessionView>,
    pub possession_binds: Vec<PossessionBind>,
    pub hooks: HookSettings,
    pub hook_recipes: Vec<HookRecipe>,
}

impl Nextbot {
    pub fn new(display_name: impl Into<String>, class_name: impl Into<String>) -> Self {
        let mut properties: BTreeMap<String, PropertyValue> = property_catalog()
            .iter()
            .map(|spec| (spec.name.to_owned(), spec.default.clone()))
            .collect();
        for (name, animation) in [
            ("WalkAnimation", "walk"),
            ("RunAnimation", "run"),
            ("IdleAnimation", "idle"),
            ("JumpAnimation", "jump"),
            ("ClimbUpAnimation", "climb"),
            ("ClimbDownAnimation", "climb"),
        ] {
            properties.insert(name.into(), PropertyValue::Text(animation.into()));
        }
        Self {
            display_name: display_name.into(),
            class_name: sanitize_class_name(&class_name.into()),
            description: String::new(),
            spawn_tab: SpawnTab::Npcs,
            custom_tab_name: "Nextbots".into(),
            category: "Nextbot".into(),
            admin_only: false,
            ignore_nextbots: true,
            base: BaseVariant::Sprite,
            properties,
            visual: VisualSettings::default(),
            audio: AudioSettings::default(),
            combat: CombatSettings::default(),
            possession_views: vec![PossessionView::default()],
            possession_binds: Vec::new(),
            hooks: HookSettings::default(),
            hook_recipes: Vec::new(),
        }
    }

    pub fn property_mut(&mut self, name: &str) -> Option<&mut PropertyValue> {
        self.properties.get_mut(name)
    }

    pub fn apply_behavior_preset(&mut self, preset: BehaviorPreset) {
        let profile = preset.profile();
        self.set_choice_property("BehaviourType", "AI_BEHAV_BASE");
        self.set_choice_property("DefaultRelationship", profile.relationship);
        self.set_bool_property("Omniscient", profile.omniscient);
        self.set_bool_property("Frightening", profile.frightening);
        self.set_number_property("SpotDuration", profile.spot_duration);
        self.set_number_property("RangeAttackRange", 0.0);
        self.set_number_property("MeleeAttackRange", profile.melee_range);
        self.set_number_property("ReachEnemyRange", profile.reach_range);
        self.set_number_property("AvoidEnemyRange", 0.0);
        self.set_number_property("AvoidAfraidOfRange", profile.avoid_afraid_range);
        self.set_number_property("WatchAfraidOfRange", profile.watch_afraid_range);
        self.set_number_property("SightFOV", profile.sight_fov);
        self.set_number_property("SightRange", profile.sight_range);
        self.set_number_property("HearingCoefficient", profile.hearing);
        self.set_integer_property("SpawnHealth", profile.health);
        self.set_number_property("HealthRegen", profile.health_regen);
        self.set_number_property("MinPhysDamage", profile.minimum_environment_damage);
        self.set_number_property("MinFallDamage", profile.minimum_environment_damage);
        self.set_number_property("Acceleration", profile.acceleration);
        self.set_number_property("Deceleration", profile.acceleration);
        self.set_number_property("JumpHeight", profile.jump_height);
        self.set_number_property("StepHeight", profile.step_height);
        self.set_number_property("MaxYawRate", profile.yaw_rate);
        self.set_number_property("DeathDropHeight", profile.death_drop_height);
        self.set_number_property("WalkSpeed", profile.walk_speed);
        self.set_number_property("RunSpeed", profile.run_speed);
        for name in [
            "ClimbLedges",
            "ClimbProps",
            "ClimbLadders",
            "ClimbLaddersUp",
            "ClimbLaddersDown",
        ] {
            self.set_bool_property(name, profile.climbs);
        }
        self.set_number_property("ClimbSpeed", profile.climb_speed);
        if profile.climbs {
            self.set_choice_property("ClimbLedgesMaxHeight", "math.huge");
            self.set_choice_property("ClimbLaddersUpMaxHeight", "math.huge");
            self.set_choice_property("ClimbLaddersDownMaxHeight", "math.huge");
            self.set_number_property("ClimbLedgesMinHeight", 0.0);
            self.set_number_property("ClimbLaddersUpMinHeight", 0.0);
            self.set_number_property("ClimbLaddersDownMinHeight", 0.0);
        }

        self.combat.melee_enabled = profile.melee_enabled;
        self.combat.melee_damage_min = profile.melee_damage;
        self.combat.melee_damage_max = profile.melee_damage;
        self.combat.melee_delay = profile.melee_delay;
        self.combat.ranged_enabled = false;
        self.hooks.patrol_when_idle = profile.patrol;
        self.hooks.spot_damage_attacker = profile.spot_damage_attacker;
    }

    fn set_bool_property(&mut self, name: &str, value: bool) {
        if let Some(PropertyValue::Bool(current)) = self.properties.get_mut(name) {
            *current = value;
        }
    }

    fn set_number_property(&mut self, name: &str, value: f64) {
        if let Some(PropertyValue::Number(current)) = self.properties.get_mut(name) {
            *current = value;
        }
    }

    fn set_integer_property(&mut self, name: &str, value: i64) {
        if let Some(PropertyValue::Integer(current)) = self.properties.get_mut(name) {
            *current = value;
        }
    }

    fn set_choice_property(&mut self, name: &str, value: &str) {
        if let Some(PropertyValue::Choice(current)) = self.properties.get_mut(name) {
            *current = value.to_owned();
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BehaviorPreset {
    Friendly,
    Aggressive,
    Hostile,
    Chase,
}

impl BehaviorPreset {
    pub const ALL: [Self; 4] = [Self::Friendly, Self::Aggressive, Self::Hostile, Self::Chase];

    pub fn label(self) -> &'static str {
        match self {
            Self::Friendly => "Friendly",
            Self::Aggressive => "Aggressive",
            Self::Hostile => "Hostile",
            Self::Chase => "Chase",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::Friendly => "Allied and non-combatant.",
            Self::Aggressive => "Neutral until provoked, then fights back.",
            Self::Hostile => "Attacks enemies on sight with balanced movement and damage.",
            Self::Chase => "Omniscient, relentless, highly durable, fast, and kills in one hit.",
        }
    }

    fn profile(self) -> BehaviorProfile {
        match self {
            Self::Friendly => BehaviorProfile {
                relationship: "D_LI",
                omniscient: false,
                frightening: false,
                spot_duration: 30.0,
                melee_range: 0.0,
                reach_range: 50.0,
                avoid_afraid_range: 500.0,
                watch_afraid_range: 750.0,
                sight_fov: 150.0,
                sight_range: 15_000.0,
                hearing: 1.0,
                health: 100,
                health_regen: 0.0,
                minimum_environment_damage: 10.0,
                acceleration: 1_000.0,
                jump_height: 50.0,
                step_height: 20.0,
                yaw_rate: 250.0,
                death_drop_height: 200.0,
                walk_speed: 100.0,
                run_speed: 200.0,
                climbs: false,
                climb_speed: 60.0,
                melee_enabled: false,
                melee_damage: 0.0,
                melee_delay: 0.3,
                patrol: true,
                spot_damage_attacker: false,
            },
            Self::Aggressive => BehaviorProfile {
                relationship: "D_NU",
                omniscient: false,
                frightening: false,
                spot_duration: 45.0,
                melee_range: 60.0,
                reach_range: 50.0,
                avoid_afraid_range: 500.0,
                watch_afraid_range: 750.0,
                sight_fov: 180.0,
                sight_range: 15_000.0,
                hearing: 1.0,
                health: 100,
                health_regen: 0.0,
                minimum_environment_damage: 10.0,
                acceleration: 1_200.0,
                jump_height: 60.0,
                step_height: 20.0,
                yaw_rate: 300.0,
                death_drop_height: 200.0,
                walk_speed: 110.0,
                run_speed: 240.0,
                climbs: false,
                climb_speed: 60.0,
                melee_enabled: true,
                melee_damage: 15.0,
                melee_delay: 0.3,
                patrol: true,
                spot_damage_attacker: true,
            },
            Self::Hostile => BehaviorProfile {
                relationship: "D_HT",
                omniscient: false,
                frightening: true,
                spot_duration: 60.0,
                melee_range: 65.0,
                reach_range: 55.0,
                avoid_afraid_range: 500.0,
                watch_afraid_range: 750.0,
                sight_fov: 180.0,
                sight_range: 20_000.0,
                hearing: 1.25,
                health: 150,
                health_regen: 0.0,
                minimum_environment_damage: 10.0,
                acceleration: 1_500.0,
                jump_height: 75.0,
                step_height: 24.0,
                yaw_rate: 360.0,
                death_drop_height: 200.0,
                walk_speed: 120.0,
                run_speed: 275.0,
                climbs: false,
                climb_speed: 75.0,
                melee_enabled: true,
                melee_damage: 25.0,
                melee_delay: 0.25,
                patrol: true,
                spot_damage_attacker: true,
            },
            Self::Chase => BehaviorProfile {
                relationship: "D_HT",
                omniscient: true,
                frightening: true,
                spot_duration: 3_600.0,
                melee_range: 100.0,
                reach_range: 90.0,
                avoid_afraid_range: 0.0,
                watch_afraid_range: 0.0,
                sight_fov: 360.0,
                sight_range: 1_000_000.0,
                hearing: 10.0,
                health: 100_000,
                health_regen: 100.0,
                minimum_environment_damage: 1_000_000.0,
                acceleration: 3_000.0,
                jump_height: 300.0,
                step_height: 35.0,
                yaw_rate: 720.0,
                death_drop_height: 1_000_000.0,
                walk_speed: 210.0,
                run_speed: 380.0,
                climbs: true,
                climb_speed: 300.0,
                melee_enabled: true,
                melee_damage: 1_000_000.0,
                melee_delay: 0.05,
                patrol: false,
                spot_damage_attacker: true,
            },
        }
    }
}

struct BehaviorProfile {
    relationship: &'static str,
    omniscient: bool,
    frightening: bool,
    spot_duration: f64,
    melee_range: f64,
    reach_range: f64,
    avoid_afraid_range: f64,
    watch_afraid_range: f64,
    sight_fov: f64,
    sight_range: f64,
    hearing: f64,
    health: i64,
    health_regen: f64,
    minimum_environment_damage: f64,
    acceleration: f64,
    jump_height: f64,
    step_height: f64,
    yaw_rate: f64,
    death_drop_height: f64,
    walk_speed: f64,
    run_speed: f64,
    climbs: bool,
    climb_speed: f64,
    melee_enabled: bool,
    melee_damage: f32,
    melee_delay: f32,
    patrol: bool,
    spot_damage_attacker: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BaseVariant {
    Standard,
    Human,
    Sprite,
}

impl BaseVariant {
    pub const ALL: [Self; 3] = [Self::Sprite, Self::Standard, Self::Human];

    pub fn label(self) -> &'static str {
        match self {
            Self::Standard => "Standard",
            Self::Human => "Human",
            Self::Sprite => "2D sprite",
        }
    }

    pub fn lua_base(self) -> &'static str {
        match self {
            Self::Standard => "drgbase_nextbot",
            Self::Human => "drgbase_nextbot_human",
            Self::Sprite => "drgbase_nextbot_sprite",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SpawnTab {
    Npcs,
    DrgBase,
    Entities,
    Custom,
}

impl SpawnTab {
    pub const ALL: [Self; 4] = [Self::Npcs, Self::DrgBase, Self::Entities, Self::Custom];

    pub fn label(&self) -> &'static str {
        match self {
            Self::Npcs => "NPCs",
            Self::DrgBase => "DrGBase",
            Self::Entities => "Entities",
            Self::Custom => "Custom tab",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VisualSettings {
    pub source: Option<PathBuf>,
    #[serde(default)]
    pub killfeed_icon: KillfeedIconSettings,
    pub material_name: String,
    pub texture_size: u32,
    pub frames_per_second: f32,
    pub width: f32,
    pub height: f32,
    pub vertical_offset: f32,
    pub translucent: bool,
    pub unlit: bool,
}

impl Default for VisualSettings {
    fn default() -> Self {
        Self {
            source: None,
            killfeed_icon: KillfeedIconSettings::default(),
            material_name: "nextbot".into(),
            texture_size: 512,
            frames_per_second: 10.0,
            width: 96.0,
            height: 128.0,
            vertical_offset: 0.0,
            translucent: true,
            unlit: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct KillfeedIconSettings {
    pub mode: KillfeedIconMode,
    pub source: Option<PathBuf>,
}

impl Default for KillfeedIconSettings {
    fn default() -> Self {
        Self {
            mode: KillfeedIconMode::NextbotSprite,
            source: None,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum KillfeedIconMode {
    NextbotSprite,
    CustomImage,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AudioSettings {
    pub spawn: Vec<PathBuf>,
    pub idle: Vec<PathBuf>,
    #[serde(default)]
    pub idle_loop: bool,
    pub damage: Vec<PathBuf>,
    pub death: Vec<PathBuf>,
    pub downed: Vec<PathBuf>,
    #[serde(default)]
    pub jump: Vec<PathBuf>,
    pub footsteps: Vec<PathBuf>,
    #[serde(default)]
    pub alert: Vec<PathBuf>,
    #[serde(default)]
    pub chase: Vec<PathBuf>,
    #[serde(default)]
    pub lost_enemy: Vec<PathBuf>,
    #[serde(default)]
    pub melee: Vec<PathBuf>,
    #[serde(default)]
    pub ranged: Vec<PathBuf>,
    #[serde(default)]
    pub land: Vec<PathBuf>,
    pub volume: f32,
    pub pitch: u16,
    pub sound_level: u16,
}

impl Default for AudioSettings {
    fn default() -> Self {
        Self {
            spawn: Vec::new(),
            idle: Vec::new(),
            idle_loop: false,
            damage: Vec::new(),
            death: Vec::new(),
            downed: Vec::new(),
            jump: Vec::new(),
            footsteps: Vec::new(),
            alert: Vec::new(),
            chase: Vec::new(),
            lost_enemy: Vec::new(),
            melee: Vec::new(),
            ranged: Vec::new(),
            land: Vec::new(),
            volume: 1.0,
            pitch: 100,
            sound_level: 75,
        }
    }
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AudioSlot {
    Spawn,
    Idle,
    Alert,
    Chase,
    LostEnemy,
    Melee,
    Ranged,
    Damage,
    Death,
    Downed,
    Jump,
    Land,
    Footsteps,
}

impl AudioSlot {
    pub const ALL: [Self; 13] = [
        Self::Spawn,
        Self::Idle,
        Self::Alert,
        Self::Chase,
        Self::LostEnemy,
        Self::Melee,
        Self::Ranged,
        Self::Damage,
        Self::Death,
        Self::Downed,
        Self::Jump,
        Self::Land,
        Self::Footsteps,
    ];

    pub fn key(self) -> &'static str {
        match self {
            Self::Spawn => "spawn",
            Self::Idle => "idle",
            Self::Alert => "alert",
            Self::Chase => "chase",
            Self::LostEnemy => "lost_enemy",
            Self::Melee => "melee",
            Self::Ranged => "ranged",
            Self::Damage => "damage",
            Self::Death => "death",
            Self::Downed => "downed",
            Self::Jump => "jump",
            Self::Land => "land",
            Self::Footsteps => "footsteps",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Spawn => "Spawn",
            Self::Idle => "Idle",
            Self::Alert => "Enemy spotted",
            Self::Chase => "Chase",
            Self::LostEnemy => "Enemy lost",
            Self::Melee => "Melee attack",
            Self::Ranged => "Ranged attack",
            Self::Damage => "Damage",
            Self::Death => "Death",
            Self::Downed => "Downed",
            Self::Jump => "Jump",
            Self::Land => "Landing",
            Self::Footsteps => "Footsteps",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::Spawn => "When the NextBot appears in the world.",
            Self::Idle => "Ambient audio, with optional continuous playback.",
            Self::Alert => "When acquiring an enemy after having no target.",
            Self::Chase => "Repeated while pursuing an enemy; stops when pursuit ends.",
            Self::LostEnemy => "When the last enemy is lost or defeated.",
            Self::Melee => "When performing a melee attack.",
            Self::Ranged => "When firing a projectile attack.",
            Self::Damage => "When taking damage, with the configured damage delay.",
            Self::Death => "When killed.",
            Self::Downed => "When entering a downed state.",
            Self::Jump => "When leaving the ground.",
            Self::Land => "When touching down after a jump or fall.",
            Self::Footsteps => "Footstep animation events on supported models.",
        }
    }

    pub fn lua_field(self) -> &'static str {
        match self {
            Self::Spawn => "OnSpawnSounds",
            Self::Idle => "OnIdleSounds",
            Self::Alert => "NBCAlertSounds",
            Self::Chase => "NBCChaseSounds",
            Self::LostEnemy => "NBCLostEnemySounds",
            Self::Melee => "NBCMeleeSounds",
            Self::Ranged => "NBCRangedSounds",
            Self::Damage => "OnDamageSounds",
            Self::Death => "OnDeathSounds",
            Self::Downed => "OnDownedSounds",
            Self::Jump => "JumpSounds",
            Self::Land => "NBCLandSounds",
            Self::Footsteps => "Footsteps",
        }
    }

    pub fn get(self, audio: &AudioSettings) -> &Vec<PathBuf> {
        match self {
            Self::Spawn => &audio.spawn,
            Self::Idle => &audio.idle,
            Self::Alert => &audio.alert,
            Self::Chase => &audio.chase,
            Self::LostEnemy => &audio.lost_enemy,
            Self::Melee => &audio.melee,
            Self::Ranged => &audio.ranged,
            Self::Damage => &audio.damage,
            Self::Death => &audio.death,
            Self::Downed => &audio.downed,
            Self::Jump => &audio.jump,
            Self::Land => &audio.land,
            Self::Footsteps => &audio.footsteps,
        }
    }

    pub fn get_mut(self, audio: &mut AudioSettings) -> &mut Vec<PathBuf> {
        match self {
            Self::Spawn => &mut audio.spawn,
            Self::Idle => &mut audio.idle,
            Self::Alert => &mut audio.alert,
            Self::Chase => &mut audio.chase,
            Self::LostEnemy => &mut audio.lost_enemy,
            Self::Melee => &mut audio.melee,
            Self::Ranged => &mut audio.ranged,
            Self::Damage => &mut audio.damage,
            Self::Death => &mut audio.death,
            Self::Downed => &mut audio.downed,
            Self::Jump => &mut audio.jump,
            Self::Land => &mut audio.land,
            Self::Footsteps => &mut audio.footsteps,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CombatSettings {
    pub melee_enabled: bool,
    pub melee_damage_min: f32,
    pub melee_damage_max: f32,
    pub melee_damage_type: String,
    pub melee_delay: f32,
    pub melee_animation: String,
    pub ranged_enabled: bool,
    pub projectile_class: String,
    pub ranged_damage: f32,
    pub ranged_speed: f32,
    pub ranged_cooldown: f32,
    pub ranged_animation: String,
}

pub const DAMAGE_TYPES: &[&str] = &[
    "DMG_GENERIC",
    "DMG_SLASH",
    "DMG_CLUB",
    "DMG_CRUSH",
    "DMG_BURN",
    "DMG_SHOCK",
    "DMG_BLAST",
    "DMG_ACID",
    "DMG_POISON",
    "DMG_DISSOLVE",
];

pub const ATTACK_ACTIVITIES: &[&str] = &[
    "ACT_MELEE_ATTACK1",
    "ACT_MELEE_ATTACK2",
    "ACT_RANGE_ATTACK1",
    "ACT_RANGE_ATTACK2",
    "ACT_SPECIAL_ATTACK1",
    "ACT_SPECIAL_ATTACK2",
    "ACT_GESTURE_RANGE_ATTACK1",
    "ACT_GESTURE_RANGE_ATTACK2",
];

pub const POSSESSION_KEYS: &[&str] = &[
    "IN_ATTACK",
    "IN_ATTACK2",
    "IN_RELOAD",
    "IN_JUMP",
    "IN_DUCK",
    "IN_USE",
    "IN_WALK",
    "IN_SPEED",
];

impl Default for CombatSettings {
    fn default() -> Self {
        Self {
            melee_enabled: true,
            melee_damage_min: 10.0,
            melee_damage_max: 15.0,
            melee_damage_type: "DMG_SLASH".into(),
            melee_delay: 0.3,
            melee_animation: "ACT_MELEE_ATTACK1".into(),
            ranged_enabled: false,
            projectile_class: "models/props_junk/PopCan01a.mdl".into(),
            ranged_damage: 10.0,
            ranged_speed: 900.0,
            ranged_cooldown: 1.0,
            ranged_animation: "ACT_RANGE_ATTACK1".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PossessionView {
    pub name: String,
    pub offset: [f32; 3],
    pub distance: f32,
    pub eye_position: bool,
}

impl Default for PossessionView {
    fn default() -> Self {
        Self {
            name: "Third person".into(),
            offset: [0.0, 30.0, 20.0],
            distance: 100.0,
            eye_position: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PossessionBind {
    pub key: String,
    pub trigger: BindTrigger,
    pub action: PossessionAction,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BindTrigger {
    Pressed,
    Held,
    Released,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PossessionAction {
    PrimaryAttack,
    SecondaryAttack,
    Reload,
    Jump,
    ToggleCrouch,
    PlaySpawnSound,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HookSettings {
    pub patrol_when_idle: bool,
    pub patrol_radius: f32,
    pub patrol_wait_min: f32,
    pub patrol_wait_max: f32,
    pub spot_damage_attacker: bool,
    pub remove_on_death: bool,
}

impl Default for HookSettings {
    fn default() -> Self {
        Self {
            patrol_when_idle: true,
            patrol_radius: 1500.0,
            patrol_wait_min: 3.0,
            patrol_wait_max: 7.0,
            spot_damage_attacker: true,
            remove_on_death: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HookRecipe {
    pub event: HookEvent,
    pub actions: Vec<HookAction>,
}

impl Default for HookRecipe {
    fn default() -> Self {
        Self {
            event: HookEvent::OnSpawn,
            actions: vec![HookAction::default()],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HookAction {
    pub kind: HookActionKind,
    pub value: f32,
}

impl Default for HookAction {
    fn default() -> Self {
        Self {
            kind: HookActionKind::PlaySpawnSound,
            value: 1.0,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum HookEvent {
    ServerInitialize,
    ServerThink,
    OnMeleeAttack,
    OnRangeAttack,
    OnChaseEnemy,
    OnAvoidEnemy,
    OnReachedPatrol,
    OnPatrolUnreachable,
    OnPatrolling,
    OnNewEnemy,
    OnEnemyChange,
    OnLastEnemy,
    OnSpawn,
    OnIdle,
    OnTakeDamage,
    OnFatalDamage,
    OnTookDamage,
    OnDeath,
    OnDowned,
    ClientInitialize,
    ClientThink,
    CustomDraw,
}

impl HookEvent {
    pub const ALL: [Self; 22] = [
        Self::ServerInitialize,
        Self::ServerThink,
        Self::OnMeleeAttack,
        Self::OnRangeAttack,
        Self::OnChaseEnemy,
        Self::OnAvoidEnemy,
        Self::OnReachedPatrol,
        Self::OnPatrolUnreachable,
        Self::OnPatrolling,
        Self::OnNewEnemy,
        Self::OnEnemyChange,
        Self::OnLastEnemy,
        Self::OnSpawn,
        Self::OnIdle,
        Self::OnTakeDamage,
        Self::OnFatalDamage,
        Self::OnTookDamage,
        Self::OnDeath,
        Self::OnDowned,
        Self::ClientInitialize,
        Self::ClientThink,
        Self::CustomDraw,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::ServerInitialize => "Server · CustomInitialize",
            Self::ServerThink => "Server · CustomThink",
            Self::OnMeleeAttack => "OnMeleeAttack",
            Self::OnRangeAttack => "OnRangeAttack",
            Self::OnChaseEnemy => "OnChaseEnemy",
            Self::OnAvoidEnemy => "OnAvoidEnemy",
            Self::OnReachedPatrol => "OnReachedPatrol",
            Self::OnPatrolUnreachable => "OnPatrolUnreachable",
            Self::OnPatrolling => "OnPatrolling",
            Self::OnNewEnemy => "OnNewEnemy",
            Self::OnEnemyChange => "OnEnemyChange",
            Self::OnLastEnemy => "OnLastEnemy",
            Self::OnSpawn => "OnSpawn",
            Self::OnIdle => "OnIdle",
            Self::OnTakeDamage => "OnTakeDamage",
            Self::OnFatalDamage => "OnFatalDamage",
            Self::OnTookDamage => "OnTookDamage",
            Self::OnDeath => "OnDeath",
            Self::OnDowned => "OnDowned",
            Self::ClientInitialize => "Client · CustomInitialize",
            Self::ClientThink => "Client · CustomThink",
            Self::CustomDraw => "Client · CustomDraw",
        }
    }

    pub fn is_client(self) -> bool {
        matches!(
            self,
            Self::ClientInitialize | Self::ClientThink | Self::CustomDraw
        )
    }

    pub fn lua_name(self) -> &'static str {
        match self {
            Self::ServerInitialize | Self::ClientInitialize => "CustomInitialize",
            Self::ServerThink | Self::ClientThink => "CustomThink",
            Self::OnMeleeAttack => "OnMeleeAttack",
            Self::OnRangeAttack => "OnRangeAttack",
            Self::OnChaseEnemy => "OnChaseEnemy",
            Self::OnAvoidEnemy => "OnAvoidEnemy",
            Self::OnReachedPatrol => "OnReachedPatrol",
            Self::OnPatrolUnreachable => "OnPatrolUnreachable",
            Self::OnPatrolling => "OnPatrolling",
            Self::OnNewEnemy => "OnNewEnemy",
            Self::OnEnemyChange => "OnEnemyChange",
            Self::OnLastEnemy => "OnLastEnemy",
            Self::OnSpawn => "OnSpawn",
            Self::OnIdle => "OnIdle",
            Self::OnTakeDamage => "OnTakeDamage",
            Self::OnFatalDamage => "OnFatalDamage",
            Self::OnTookDamage => "OnTookDamage",
            Self::OnDeath => "OnDeath",
            Self::OnDowned => "OnDowned",
            Self::CustomDraw => "CustomDraw",
        }
    }

    pub fn lua_parameters(self) -> &'static str {
        match self {
            Self::OnMeleeAttack | Self::OnRangeAttack => "enemy, weapon",
            Self::OnChaseEnemy | Self::OnAvoidEnemy | Self::OnNewEnemy | Self::OnLastEnemy => {
                "enemy"
            }
            Self::OnReachedPatrol | Self::OnPatrolUnreachable | Self::OnPatrolling => {
                "position, patrol"
            }
            Self::OnEnemyChange => "oldEnemy, newEnemy",
            Self::OnTakeDamage
            | Self::OnFatalDamage
            | Self::OnTookDamage
            | Self::OnDeath
            | Self::OnDowned => "damage, hitgroup",
            _ => "",
        }
    }

    pub fn related_entity(self) -> &'static str {
        match self {
            Self::OnMeleeAttack
            | Self::OnRangeAttack
            | Self::OnChaseEnemy
            | Self::OnAvoidEnemy
            | Self::OnNewEnemy
            | Self::OnLastEnemy => "enemy",
            Self::OnEnemyChange => "newEnemy",
            Self::OnTakeDamage
            | Self::OnFatalDamage
            | Self::OnTookDamage
            | Self::OnDeath
            | Self::OnDowned => "damage:GetAttacker()",
            _ => "self:GetEnemy()",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HookActionKind {
    Wait,
    AddRandomPatrol,
    PlaySpawnSound,
    PlayIdleSound,
    PlayDamageSound,
    SpotRelatedEntity,
    SetEnemyToRelated,
    ClearEnemy,
    Heal,
    DisableAi,
    EnableAi,
    PerformMeleeAttack,
    PerformRangeAttack,
    RemoveSelf,
}

impl HookActionKind {
    pub const ALL: [Self; 14] = [
        Self::Wait,
        Self::AddRandomPatrol,
        Self::PlaySpawnSound,
        Self::PlayIdleSound,
        Self::PlayDamageSound,
        Self::SpotRelatedEntity,
        Self::SetEnemyToRelated,
        Self::ClearEnemy,
        Self::Heal,
        Self::DisableAi,
        Self::EnableAi,
        Self::PerformMeleeAttack,
        Self::PerformRangeAttack,
        Self::RemoveSelf,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Wait => "Wait",
            Self::AddRandomPatrol => "Add random patrol point",
            Self::PlaySpawnSound => "Play spawn sound",
            Self::PlayIdleSound => "Play idle sound",
            Self::PlayDamageSound => "Play damage sound",
            Self::SpotRelatedEntity => "Spot related entity",
            Self::SetEnemyToRelated => "Set related entity as enemy",
            Self::ClearEnemy => "Clear enemy",
            Self::Heal => "Heal",
            Self::DisableAi => "Disable AI",
            Self::EnableAi => "Enable AI",
            Self::PerformMeleeAttack => "Perform melee attack",
            Self::PerformRangeAttack => "Perform ranged attack",
            Self::RemoveSelf => "Remove NextBot",
        }
    }

    pub fn uses_value(self) -> bool {
        matches!(self, Self::Wait | Self::AddRandomPatrol | Self::Heal)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum PropertyValue {
    Bool(bool),
    Number(f64),
    Integer(i64),
    Text(String),
    StringList(Vec<String>),
    IntegerList(Vec<i64>),
    Vector([f64; 3]),
    Angle([f64; 3]),
    Choice(String),
}

impl PropertyValue {
    pub fn to_lua(&self) -> String {
        match self {
            Self::Bool(value) => value.to_string(),
            Self::Number(value) => format_number(*value),
            Self::Integer(value) => value.to_string(),
            Self::Text(value) => lua_string(value),
            Self::StringList(values) => format!(
                "{{{}}}",
                values
                    .iter()
                    .map(|value| lua_string(value))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Self::IntegerList(values) => format!(
                "{{{}}}",
                values
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Self::Vector(value) => format!(
                "Vector({}, {}, {})",
                format_number(value[0]),
                format_number(value[1]),
                format_number(value[2])
            ),
            Self::Angle(value) => format!(
                "Angle({}, {}, {})",
                format_number(value[0]),
                format_number(value[1]),
                format_number(value[2])
            ),
            Self::Choice(value) => value.clone(),
        }
    }
}

pub fn format_number(value: f64) -> String {
    if value.fract() == 0.0 {
        format!("{value:.0}")
    } else {
        let rendered = format!("{value:.6}");
        rendered.trim_end_matches('0').to_owned()
    }
}

pub fn lua_string(value: &str) -> String {
    format!(
        "\"{}\"",
        value
            .replace('\\', "\\\\")
            .replace('\"', "\\\"")
            .replace('\r', "\\r")
            .replace('\n', "\\n")
    )
}

pub fn slugify(value: &str) -> String {
    let value = value.trim().to_ascii_lowercase();
    let mut result = String::with_capacity(value.len());
    let mut separator = false;
    for character in value.chars() {
        if character.is_ascii_alphanumeric() {
            result.push(character);
            separator = false;
        } else if !separator && !result.is_empty() {
            result.push('_');
            separator = true;
        }
    }
    result.trim_matches('_').to_owned()
}

pub fn sanitize_class_name(value: &str) -> String {
    let slug = slugify(value);
    if slug.is_empty() {
        "npc_nextbot".into()
    } else {
        slug
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs_are_safe_for_addon_and_entity_paths() {
        assert_eq!(slugify(" My NextBot 01! "), "my_nextbot_01");
        assert_eq!(sanitize_class_name(""), "npc_nextbot");
    }

    #[test]
    fn lua_strings_escape_code_delimiters() {
        assert_eq!(lua_string("a\"b\\c\n"), "\"a\\\"b\\\\c\\n\"");
    }

    #[test]
    fn chase_preset_is_relentless_and_friendly_resets_it() {
        let mut bot = Nextbot::new("Preset Test", "npc_preset_test");
        bot.apply_behavior_preset(BehaviorPreset::Chase);
        assert_eq!(
            bot.properties.get("DefaultRelationship"),
            Some(&PropertyValue::Choice("D_HT".into()))
        );
        assert_eq!(
            bot.properties.get("Omniscient"),
            Some(&PropertyValue::Bool(true))
        );
        assert_eq!(bot.combat.melee_damage_min, 1_000_000.0);
        assert_eq!(
            bot.properties.get("RunSpeed"),
            Some(&PropertyValue::Number(380.0))
        );
        assert!(!bot.hooks.patrol_when_idle);

        bot.apply_behavior_preset(BehaviorPreset::Friendly);
        assert_eq!(
            bot.properties.get("DefaultRelationship"),
            Some(&PropertyValue::Choice("D_LI".into()))
        );
        assert_eq!(
            bot.properties.get("SpawnHealth"),
            Some(&PropertyValue::Integer(100))
        );
        assert!(!bot.combat.melee_enabled);
        assert!(bot.hooks.patrol_when_idle);
    }

    #[test]
    fn projects_without_new_media_fields_still_deserialize() {
        let project = Project::new("Legacy", PathBuf::from("legacy"));
        let mut value = serde_json::to_value(project).unwrap();
        let bot = value["nextbots"][0].as_object_mut().unwrap();
        bot["visual"]
            .as_object_mut()
            .unwrap()
            .remove("killfeed_icon");
        bot["audio"].as_object_mut().unwrap().remove("jump");
        bot["audio"].as_object_mut().unwrap().remove("idle_loop");
        bot.remove("ignore_nextbots");
        for key in ["alert", "chase", "lost_enemy", "melee", "ranged", "land"] {
            bot["audio"].as_object_mut().unwrap().remove(key);
        }

        let loaded: Project = serde_json::from_value(value).unwrap();
        assert_eq!(
            loaded.nextbots[0].visual.killfeed_icon,
            KillfeedIconSettings::default()
        );
        assert!(loaded.nextbots[0].audio.jump.is_empty());
        assert!(!loaded.nextbots[0].audio.idle_loop);
        assert!(loaded.nextbots[0].ignore_nextbots);
        for slot in AudioSlot::ALL {
            assert!(slot.get(&loaded.nextbots[0].audio).is_empty());
        }
    }

    #[test]
    fn explicit_nextbot_relationship_choice_survives_presets_and_round_trip() {
        let mut bot = Nextbot::new("Hunter", "npc_hunter");
        assert!(bot.ignore_nextbots);
        bot.ignore_nextbots = false;
        for preset in BehaviorPreset::ALL {
            bot.apply_behavior_preset(preset);
            assert!(!bot.ignore_nextbots);
        }
        let loaded: Nextbot = serde_json::from_str(&serde_json::to_string(&bot).unwrap()).unwrap();
        assert!(!loaded.ignore_nextbots);
    }

    #[test]
    fn class_names_stay_unique_after_repeated_duplicates_and_removal() {
        let mut project = Project::new("Pack", PathBuf::from("pack"));
        for _ in 0..3 {
            let class = project.unique_class_name("npc_my_nextbot_copy");
            project.nextbots.push(Nextbot::new("Copy", class));
        }
        assert_eq!(project.nextbots[3].class_name, "npc_my_nextbot_copy_3");
        project.nextbots.remove(2);
        assert_eq!(
            project.unique_class_name("npc_my_nextbot_copy"),
            "npc_my_nextbot_copy_2"
        );
    }
}
