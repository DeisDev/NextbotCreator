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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AudioSettings {
    pub spawn: Vec<PathBuf>,
    pub idle: Vec<PathBuf>,
    pub damage: Vec<PathBuf>,
    pub death: Vec<PathBuf>,
    pub downed: Vec<PathBuf>,
    pub footsteps: Vec<PathBuf>,
    pub volume: f32,
    pub pitch: u16,
    pub sound_level: u16,
}

impl Default for AudioSettings {
    fn default() -> Self {
        Self {
            spawn: Vec::new(),
            idle: Vec::new(),
            damage: Vec::new(),
            death: Vec::new(),
            downed: Vec::new(),
            footsteps: Vec::new(),
            volume: 1.0,
            pitch: 100,
            sound_level: 75,
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
}
