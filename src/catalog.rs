use crate::domain::PropertyValue;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PropertySection {
    Identity,
    Appearance,
    Stats,
    Ai,
    Relationships,
    Detection,
    Locomotion,
    Movement,
    Climbing,
    Animation,
    Sounds,
    Weapons,
    Possession,
    Sprite,
}

impl PropertySection {
    pub const ALL: [Self; 14] = [
        Self::Identity,
        Self::Appearance,
        Self::Stats,
        Self::Ai,
        Self::Relationships,
        Self::Detection,
        Self::Locomotion,
        Self::Movement,
        Self::Climbing,
        Self::Animation,
        Self::Sounds,
        Self::Weapons,
        Self::Possession,
        Self::Sprite,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Identity => "Identity",
            Self::Appearance => "Appearance",
            Self::Stats => "Stats",
            Self::Ai => "AI & combat ranges",
            Self::Relationships => "Relationships",
            Self::Detection => "Detection",
            Self::Locomotion => "Locomotion",
            Self::Movement => "Movement",
            Self::Climbing => "Climbing",
            Self::Animation => "Animation",
            Self::Sounds => "Sounds",
            Self::Weapons => "Weapons",
            Self::Possession => "Possession",
            Self::Sprite => "Sprite",
        }
    }
}

#[derive(Debug, Clone)]
pub struct PropertySpec {
    pub name: &'static str,
    pub label: &'static str,
    pub help: &'static str,
    pub section: PropertySection,
    pub default: PropertyValue,
    pub choices: &'static [&'static str],
    pub basic: bool,
}

macro_rules! spec {
    ($name:literal, $label:literal, $help:literal, $section:ident, $default:expr) => {
        PropertySpec {
            name: $name,
            label: $label,
            help: $help,
            section: PropertySection::$section,
            default: $default,
            choices: &[],
            basic: false,
        }
    };
    ($name:literal, $label:literal, $help:literal, $section:ident, $default:expr, basic) => {
        PropertySpec {
            name: $name,
            label: $label,
            help: $help,
            section: PropertySection::$section,
            default: $default,
            choices: &[],
            basic: true,
        }
    };
    ($name:literal, $label:literal, $help:literal, $section:ident, $default:expr, choices = $choices:expr) => {
        PropertySpec {
            name: $name,
            label: $label,
            help: $help,
            section: PropertySection::$section,
            default: $default,
            choices: $choices,
            basic: false,
        }
    };
    ($name:literal, $label:literal, $help:literal, $section:ident, $default:expr, choices = $choices:expr, basic) => {
        PropertySpec {
            name: $name,
            label: $label,
            help: $help,
            section: PropertySection::$section,
            default: $default,
            choices: $choices,
            basic: true,
        }
    };
}

pub fn property_catalog() -> Vec<PropertySpec> {
    use PropertyValue::*;
    vec![
        spec!(
            "Models",
            "Models",
            "Source model paths used at spawn.",
            Appearance,
            StringList(vec!["models/props_lab/blastdoor001a.mdl".into()]),
            basic
        ),
        spec!(
            "Skins",
            "Skins",
            "Allowed model skin indexes.",
            Appearance,
            IntegerList(vec![0])
        ),
        spec!(
            "ModelScale",
            "Model scale",
            "Uniform model scale.",
            Appearance,
            Number(1.0),
            basic
        ),
        spec!(
            "CollisionBounds",
            "Collision bounds",
            "Half-width, half-depth and height.",
            Appearance,
            Vector([10.0, 10.0, 72.0]),
            basic
        ),
        spec!(
            "BloodColor",
            "Blood color",
            "Engine blood color constant.",
            Appearance,
            Choice("BLOOD_COLOR_RED".into()),
            choices = &[
                "BLOOD_COLOR_RED",
                "BLOOD_COLOR_YELLOW",
                "BLOOD_COLOR_GREEN",
                "BLOOD_COLOR_MECH",
                "DONT_BLEED"
            ]
        ),
        spec!(
            "RagdollOnDeath",
            "Ragdoll on death",
            "Create a ragdoll when killed.",
            Appearance,
            Bool(true),
            basic
        ),
        spec!(
            "SpawnHealth",
            "Spawn health",
            "Maximum health at spawn.",
            Stats,
            Integer(100),
            basic
        ),
        spec!(
            "HealthRegen",
            "Health regeneration",
            "Health restored per second.",
            Stats,
            Number(0.0),
            basic
        ),
        spec!(
            "MinPhysDamage",
            "Minimum physics damage",
            "Ignore smaller physics hits.",
            Stats,
            Number(10.0)
        ),
        spec!(
            "MinFallDamage",
            "Minimum fall damage",
            "Ignore smaller fall hits.",
            Stats,
            Number(10.0)
        ),
        spec!(
            "BehaviourType",
            "Behaviour type",
            "Built-in DRGBase AI behaviour.",
            Ai,
            Choice("AI_BEHAV_BASE".into()),
            choices = &["AI_BEHAV_BASE", "AI_BEHAV_HUMAN"],
            basic
        ),
        spec!(
            "Omniscient",
            "Omniscient",
            "Always know relevant entity positions.",
            Ai,
            Bool(false)
        ),
        spec!(
            "SpotDuration",
            "Spot duration",
            "Seconds an entity remains spotted.",
            Ai,
            Number(30.0),
            basic
        ),
        spec!(
            "RangeAttackRange",
            "Range attack range",
            "Maximum ranged-attack distance; zero disables it.",
            Ai,
            Number(0.0),
            basic
        ),
        spec!(
            "MeleeAttackRange",
            "Melee attack range",
            "Maximum melee-attack distance; zero disables it.",
            Ai,
            Number(50.0),
            basic
        ),
        spec!(
            "ReachEnemyRange",
            "Reach enemy range",
            "Distance considered close enough to an enemy.",
            Ai,
            Number(50.0)
        ),
        spec!(
            "AvoidEnemyRange",
            "Avoid enemy range",
            "Distance maintained from enemies.",
            Ai,
            Number(0.0)
        ),
        spec!(
            "AvoidAfraidOfRange",
            "Fear avoidance range",
            "Distance maintained from frightening entities.",
            Ai,
            Number(500.0)
        ),
        spec!(
            "WatchAfraidOfRange",
            "Fear watch range",
            "Distance at which frightening entities are watched.",
            Ai,
            Number(750.0)
        ),
        spec!(
            "DefaultRelationship",
            "Default disposition",
            "Disposition toward otherwise unknown entities.",
            Relationships,
            Choice("D_NU".into()),
            choices = &["D_HT", "D_FR", "D_LI", "D_NU"]
        ),
        spec!(
            "Factions",
            "Factions",
            "Faction constants or names inherited by this NextBot.",
            Relationships,
            StringList(Vec::new()),
            basic
        ),
        spec!(
            "Frightening",
            "Frightening",
            "Other entities may fear this NextBot.",
            Relationships,
            Bool(false)
        ),
        spec!(
            "AllyDamageTolerance",
            "Ally damage tolerance",
            "Fraction of health allies may inflict before hostility.",
            Relationships,
            Number(0.33)
        ),
        spec!(
            "AfraidDamageTolerance",
            "Fear damage tolerance",
            "Fraction of health feared entities may inflict.",
            Relationships,
            Number(0.33)
        ),
        spec!(
            "NeutralDamageTolerance",
            "Neutral damage tolerance",
            "Fraction of health neutral entities may inflict.",
            Relationships,
            Number(0.33)
        ),
        spec!(
            "EyeBone",
            "Eye bone",
            "Model bone used as the eye origin.",
            Detection,
            Text(String::new())
        ),
        spec!(
            "EyeOffset",
            "Eye offset",
            "Offset from the selected eye bone.",
            Detection,
            Vector([0.0, 0.0, 0.0])
        ),
        spec!(
            "EyeAngle",
            "Eye angle",
            "Rotation applied to the eye direction.",
            Detection,
            Angle([0.0, 0.0, 0.0])
        ),
        spec!(
            "SightFOV",
            "Sight FOV",
            "Horizontal field of view in degrees.",
            Detection,
            Number(150.0),
            basic
        ),
        spec!(
            "SightRange",
            "Sight range",
            "Maximum visual detection distance.",
            Detection,
            Number(15000.0),
            basic
        ),
        spec!(
            "MinLuminosity",
            "Minimum luminosity",
            "Darkest detectable normalized light level.",
            Detection,
            Number(0.0)
        ),
        spec!(
            "MaxLuminosity",
            "Maximum luminosity",
            "Brightest detectable normalized light level.",
            Detection,
            Number(1.0)
        ),
        spec!(
            "HearingCoefficient",
            "Hearing coefficient",
            "Hearing multiplier; zero is deaf.",
            Detection,
            Number(1.0),
            basic
        ),
        spec!(
            "Acceleration",
            "Acceleration",
            "Locomotor acceleration.",
            Locomotion,
            Number(1000.0),
            basic
        ),
        spec!(
            "Deceleration",
            "Deceleration",
            "Locomotor braking rate.",
            Locomotion,
            Number(1000.0)
        ),
        spec!(
            "JumpHeight",
            "Jump height",
            "Maximum automatic jump height.",
            Locomotion,
            Number(50.0),
            basic
        ),
        spec!(
            "StepHeight",
            "Step height",
            "Maximum traversable step height.",
            Locomotion,
            Number(20.0)
        ),
        spec!(
            "MaxYawRate",
            "Maximum yaw rate",
            "Maximum turn rate in degrees per second.",
            Locomotion,
            Number(250.0)
        ),
        spec!(
            "DeathDropHeight",
            "Death drop height",
            "Fall distance considered fatal by the base.",
            Locomotion,
            Number(200.0)
        ),
        spec!(
            "UseWalkframes",
            "Use walk frames",
            "Drive movement from animation ground speed.",
            Movement,
            Bool(false)
        ),
        spec!(
            "WalkSpeed",
            "Walk speed",
            "Walking units per second.",
            Movement,
            Number(100.0),
            basic
        ),
        spec!(
            "RunSpeed",
            "Run speed",
            "Running units per second.",
            Movement,
            Number(200.0),
            basic
        ),
        spec!(
            "ClimbLedges",
            "Climb ledges",
            "Enable ledge climbing.",
            Climbing,
            Bool(false),
            basic
        ),
        spec!(
            "ClimbLedgesMaxHeight",
            "Maximum ledge height",
            "Maximum climbable ledge height.",
            Climbing,
            Choice("math.huge".into()),
            choices = &["math.huge", "200", "100", "50"]
        ),
        spec!(
            "ClimbLedgesMinHeight",
            "Minimum ledge height",
            "Minimum ledge height that triggers climbing.",
            Climbing,
            Number(0.0)
        ),
        spec!(
            "LedgeDetectionDistance",
            "Ledge detection distance",
            "Forward ledge probe distance.",
            Climbing,
            Number(20.0)
        ),
        spec!(
            "ClimbProps",
            "Climb props",
            "Allow climbing over props.",
            Climbing,
            Bool(false)
        ),
        spec!(
            "ClimbLadders",
            "Climb ladders",
            "Enable ladder navigation.",
            Climbing,
            Bool(false),
            basic
        ),
        spec!(
            "ClimbLaddersUp",
            "Climb ladders up",
            "Allow upward ladder traversal.",
            Climbing,
            Bool(true)
        ),
        spec!(
            "LaddersUpDistance",
            "Up-ladder detection distance",
            "Forward probe for upward ladders.",
            Climbing,
            Number(20.0)
        ),
        spec!(
            "ClimbLaddersUpMaxHeight",
            "Maximum upward ladder height",
            "Maximum upward ladder traversal.",
            Climbing,
            Choice("math.huge".into()),
            choices = &["math.huge", "500", "200", "100"]
        ),
        spec!(
            "ClimbLaddersUpMinHeight",
            "Minimum upward ladder height",
            "Minimum upward ladder traversal.",
            Climbing,
            Number(0.0)
        ),
        spec!(
            "ClimbLaddersDown",
            "Climb ladders down",
            "Allow downward ladder traversal.",
            Climbing,
            Bool(false)
        ),
        spec!(
            "LaddersDownDistance",
            "Down-ladder detection distance",
            "Forward probe for downward ladders.",
            Climbing,
            Number(20.0)
        ),
        spec!(
            "ClimbLaddersDownMaxHeight",
            "Maximum downward ladder height",
            "Maximum downward ladder traversal.",
            Climbing,
            Choice("math.huge".into()),
            choices = &["math.huge", "500", "200", "100"]
        ),
        spec!(
            "ClimbLaddersDownMinHeight",
            "Minimum downward ladder height",
            "Minimum downward ladder traversal.",
            Climbing,
            Number(0.0)
        ),
        spec!(
            "ClimbSpeed",
            "Climb speed",
            "Units per second while climbing.",
            Climbing,
            Number(60.0)
        ),
        spec!(
            "ClimbUpAnimation",
            "Climb-up animation",
            "Activity or sprite animation name.",
            Climbing,
            Choice("ACT_CLIMB_UP".into()),
            choices = &["ACT_CLIMB_UP", "ACT_ZOMBIE_CLIMB_UP", "ACT_JUMP"]
        ),
        spec!(
            "ClimbDownAnimation",
            "Climb-down animation",
            "Activity or sprite animation name.",
            Climbing,
            Choice("ACT_CLIMB_DOWN".into()),
            choices = &["ACT_CLIMB_DOWN", "ACT_ZOMBIE_CLIMB_UP", "ACT_JUMP"]
        ),
        spec!(
            "ClimbAnimRate",
            "Climb animation rate",
            "Climb animation playback multiplier.",
            Climbing,
            Number(1.0)
        ),
        spec!(
            "ClimbOffset",
            "Climb offset",
            "Model alignment while climbing.",
            Climbing,
            Vector([0.0, 0.0, 0.0])
        ),
        spec!(
            "WalkAnimation",
            "Walk animation",
            "Walk activity or sprite animation.",
            Animation,
            Choice("ACT_WALK".into()),
            choices = &["ACT_WALK", "ACT_WALK_AIM", "ACT_WALK_CROUCH"]
        ),
        spec!(
            "WalkAnimRate",
            "Walk animation rate",
            "Walk animation playback multiplier.",
            Animation,
            Number(1.0)
        ),
        spec!(
            "RunAnimation",
            "Run animation",
            "Run activity or sprite animation.",
            Animation,
            Choice("ACT_RUN".into()),
            choices = &["ACT_RUN", "ACT_RUN_AIM", "ACT_RUN_CROUCH"]
        ),
        spec!(
            "RunAnimRate",
            "Run animation rate",
            "Run animation playback multiplier.",
            Animation,
            Number(1.0)
        ),
        spec!(
            "IdleAnimation",
            "Idle animation",
            "Idle activity or sprite animation.",
            Animation,
            Choice("ACT_IDLE".into()),
            choices = &["ACT_IDLE", "ACT_IDLE_ANGRY", "ACT_IDLE_RELAXED"]
        ),
        spec!(
            "IdleAnimRate",
            "Idle animation rate",
            "Idle animation playback multiplier.",
            Animation,
            Number(1.0)
        ),
        spec!(
            "JumpAnimation",
            "Jump animation",
            "Jump activity or sprite animation.",
            Animation,
            Choice("ACT_JUMP".into()),
            choices = &["ACT_JUMP", "ACT_GLIDE"]
        ),
        spec!(
            "JumpAnimRate",
            "Jump animation rate",
            "Jump animation playback multiplier.",
            Animation,
            Number(1.0)
        ),
        spec!(
            "IdleSoundDelay",
            "Idle sound delay",
            "Minimum seconds between idle sounds.",
            Sounds,
            Number(2.0),
            basic
        ),
        spec!(
            "ClientIdleSounds",
            "Client idle sounds",
            "Allow idle sounds to originate client-side.",
            Sounds,
            Bool(false)
        ),
        spec!(
            "DamageSoundDelay",
            "Damage sound delay",
            "Minimum seconds between damage sounds.",
            Sounds,
            Number(0.25)
        ),
        spec!(
            "UseWeapons",
            "Use weapons",
            "Allow Source weapons.",
            Weapons,
            Bool(false),
            basic
        ),
        spec!(
            "Weapons",
            "Weapons",
            "Weapon class names available at spawn.",
            Weapons,
            StringList(Vec::new())
        ),
        spec!(
            "WeaponAccuracy",
            "Weapon accuracy",
            "Weapon aim multiplier documented by DRGBase.",
            Weapons,
            Number(1.0)
        ),
        spec!(
            "WeaponAttachment",
            "Weapon attachment",
            "Model attachment used to hold weapons.",
            Weapons,
            Text("Anim_Attachment_RH".into())
        ),
        spec!(
            "DropWeaponOnDeath",
            "Drop weapon on death",
            "Drop the active weapon when killed.",
            Weapons,
            Bool(false)
        ),
        spec!(
            "AcceptPlayerWeapons",
            "Accept player weapons",
            "Allow players to give weapons.",
            Weapons,
            Bool(true)
        ),
        spec!(
            "PossessionEnabled",
            "Enable possession",
            "Allow players to possess this NextBot.",
            Possession,
            Bool(false),
            basic
        ),
        spec!(
            "PossessionPrompt",
            "Possession prompt",
            "Add spawn-and-possess context action.",
            Possession,
            Bool(true)
        ),
        spec!(
            "PossessionCrosshair",
            "Possession crosshair",
            "Draw a crosshair while possessed.",
            Possession,
            Bool(false)
        ),
        spec!(
            "PossessionMovement",
            "Possession movement",
            "Possessed movement scheme.",
            Possession,
            Choice("POSSESSION_MOVE_1DIR".into()),
            choices = &[
                "POSSESSION_MOVE_1DIR",
                "POSSESSION_MOVE_4DIR",
                "POSSESSION_MOVE_8DIR",
                "POSSESSION_MOVE_ANALOG"
            ]
        ),
        spec!(
            "SpriteFolder",
            "Sprite folder",
            "DRGBase sprite animation folder.",
            Sprite,
            Text(String::new())
        ),
        spec!(
            "FramesPerSecond",
            "Sprite frames per second",
            "DRGBase sprite animation rate.",
            Sprite,
            Number(10.0)
        ),
        spec!(
            "RenderGroup",
            "Render group",
            "Client render-group constant.",
            Sprite,
            Choice("RENDERGROUP_TRANSLUCENT".into()),
            choices = &["RENDERGROUP_TRANSLUCENT", "RENDERGROUP_OPAQUE"]
        ),
    ]
}

pub const DRGBASE_BASELINE_COMMIT: &str = "1f197add9a755a474eafdd6c7a277be1bdac27be";

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn catalog_names_are_unique() {
        let catalog = property_catalog();
        let unique = catalog
            .iter()
            .map(|field| field.name)
            .collect::<HashSet<_>>();
        assert_eq!(catalog.len(), unique.len());
        assert!(
            catalog.len() >= 75,
            "catalog unexpectedly lost DRGBase coverage"
        );
    }
}
