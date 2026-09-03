use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::catalog::{DRGBASE_BASELINE_COMMIT, property_catalog};
use crate::converter::{self, ConversionError};
use crate::domain::{
    ATTACK_ACTIVITIES, BindTrigger, DAMAGE_TYPES, HookActionKind, HookEvent, Nextbot,
    POSSESSION_KEYS, PossessionAction, Project, PropertyValue, SpawnTab, format_number, lua_string,
    sanitize_class_name, slugify,
};
use crate::{APP_VERSION, PROJECT_FILE};

const GENERATED_MANIFEST: &str = ".nextbotcreator-generated.json";

#[derive(Debug, Error)]
pub enum GenerationError {
    #[error("project validation failed:\n{0}")]
    Validation(String),
    #[error(transparent)]
    Conversion(#[from] ConversionError),
    #[error("I/O error at {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("failed to serialize generated-file manifest: {0}")]
    Manifest(#[from] serde_json::Error),
}

#[derive(Debug, Clone)]
pub struct GenerationReport {
    pub files_written: usize,
    pub files_removed: usize,
    pub warnings: Vec<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct GeneratedManifest {
    version: String,
    files: BTreeSet<PathBuf>,
}

pub fn validate_project(project: &Project) -> Result<Vec<String>, GenerationError> {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    if project.name.trim().is_empty() {
        errors.push("Project name is empty.".to_owned());
    }
    if project.slug.trim().is_empty() || project.slug != slugify(&project.slug) {
        errors.push(
            "Project folder name may only use lowercase letters, numbers, and underscores."
                .to_owned(),
        );
    }
    if project.nextbots.is_empty() {
        errors.push("Add at least one NextBot.".to_owned());
    }
    let mut classes = BTreeSet::new();
    for bot in &project.nextbots {
        if bot.display_name.trim().is_empty() {
            errors.push("A NextBot has no display name.".to_owned());
        }
        if bot.class_name != sanitize_class_name(&bot.class_name) {
            errors.push(format!(
                "Class '{}' may only use lowercase letters, numbers, and underscores.",
                bot.class_name
            ));
        }
        if !classes.insert(bot.class_name.clone()) {
            errors.push(format!(
                "Class '{}' is used more than once.",
                bot.class_name
            ));
        }
        if bot.category.trim().is_empty() {
            errors.push(format!("{} has no spawnmenu category.", bot.display_name));
        }
        if bot.visual.material_name.trim().is_empty()
            || bot.visual.material_name != slugify(&bot.visual.material_name)
        {
            errors.push(format!(
                "{} has an invalid material name; use lowercase letters, numbers, and underscores.",
                bot.display_name
            ));
        }
        if matches!(bot.spawn_tab, SpawnTab::Custom) && bot.custom_tab_name.trim().is_empty() {
            errors.push(format!(
                "{} uses a custom tab but has no tab name.",
                bot.display_name
            ));
        }
        if matches!(bot.base, crate::domain::BaseVariant::Sprite) && bot.visual.source.is_none() {
            warnings.push(format!(
                "{} has no visual asset and will use the base placeholder.",
                bot.display_name
            ));
        }
        if bot.visual.texture_size > 2048 {
            warnings.push(format!(
                "{} uses a texture larger than 2048px; this can increase loading time.",
                bot.display_name
            ));
        }
        if !DAMAGE_TYPES.contains(&bot.combat.melee_damage_type.as_str()) {
            errors.push(format!(
                "{} has an invalid melee damage type; choose a value from the GUI.",
                bot.display_name
            ));
        }
        for (label, value) in [
            ("melee animation", bot.combat.melee_animation.as_str()),
            ("ranged animation", bot.combat.ranged_animation.as_str()),
        ] {
            if !ATTACK_ACTIVITIES.contains(&value) {
                errors.push(format!(
                    "{} has an invalid {label}; choose a value from the GUI.",
                    bot.display_name
                ));
            }
        }
        for bind in &bot.possession_binds {
            if !POSSESSION_KEYS.contains(&bind.key.as_str()) {
                errors.push(format!(
                    "{} has an invalid possession key; choose a value from the GUI.",
                    bot.display_name
                ));
            }
        }
        for spec in property_catalog() {
            let Some(value) = bot.properties.get(spec.name) else {
                errors.push(format!(
                    "{} is missing the documented {} setting.",
                    bot.display_name, spec.name
                ));
                continue;
            };
            if let PropertyValue::Choice(choice) = value
                && !spec.choices.contains(&choice.as_str())
            {
                errors.push(format!(
                    "{} has an invalid {} choice; choose a value from the GUI.",
                    bot.display_name, spec.label
                ));
            }
            if !property_numbers_are_finite(value) {
                errors.push(format!(
                    "{} has a non-finite value for {}.",
                    bot.display_name, spec.label
                ));
            }
        }
    }
    if errors.is_empty() {
        Ok(warnings)
    } else {
        Err(GenerationError::Validation(errors.join("\n")))
    }
}

fn property_numbers_are_finite(value: &PropertyValue) -> bool {
    match value {
        PropertyValue::Number(value) => value.is_finite(),
        PropertyValue::Vector(value) | PropertyValue::Angle(value) => {
            value.iter().all(|component| component.is_finite())
        }
        _ => true,
    }
}

pub fn generate_project(
    project: &Project,
    portable_root: &Path,
) -> Result<GenerationReport, GenerationError> {
    let mut warnings = validate_project(project)?;
    fs::create_dir_all(&project.root).map_err(|source| GenerationError::Io {
        path: project.root.clone(),
        source,
    })?;
    let mut generated = BTreeSet::new();
    let mut written = 0;

    let addon_json = serde_json::json!({
        "title": project.name,
        "type": "npc",
        "tags": ["fun"],
        "ignore": [PROJECT_FILE, "source_assets", GENERATED_MANIFEST]
    });
    write_generated(
        project,
        Path::new("addon.json"),
        serde_json::to_vec_pretty(&addon_json)?,
        &mut generated,
        &mut written,
    )?;

    let mut sound_entries = String::new();
    let mut custom_tabs: BTreeMap<String, Vec<&Nextbot>> = BTreeMap::new();

    for bot in &project.nextbots {
        let material_relative = format!(
            "nextbotcreator/{}/{}/{}",
            project.slug, bot.class_name, bot.visual.material_name
        );
        let mut visual_available = false;
        if let Some(source) = &bot.visual.source {
            if source.is_file() {
                let artifact = converter::convert_visual(
                    source,
                    &project.root.join("materials"),
                    &project.root.join("materials").join("entities"),
                    &material_relative,
                    &bot.class_name,
                    &bot.visual,
                )?;
                for path in [&artifact.vtf_path, &artifact.vmt_path, &artifact.icon_path] {
                    if let Ok(relative) = path.strip_prefix(&project.root) {
                        generated.insert(relative.to_path_buf());
                    }
                }
                visual_available = true;
                written += 3;
                if artifact.frame_count > 1 {
                    warnings.push(format!(
                        "{}: encoded {} animation frames at {} FPS.",
                        bot.display_name, artifact.frame_count, bot.visual.frames_per_second
                    ));
                }
            } else {
                warnings.push(format!(
                    "{}: visual source is missing: {}",
                    bot.display_name,
                    source.display()
                ));
            }
        }

        let sound_names =
            convert_bot_audio(project, bot, portable_root, &mut generated, &mut written)?;
        sound_entries.push_str(&render_sound_entries(bot, &sound_names));

        let entity_folder = PathBuf::from("lua").join("entities").join(&bot.class_name);
        write_generated(
            project,
            &entity_folder.join("shared.lua"),
            render_shared(project, bot, &sound_names).into_bytes(),
            &mut generated,
            &mut written,
        )?;
        write_generated(
            project,
            &entity_folder.join("init.lua"),
            render_server(bot).into_bytes(),
            &mut generated,
            &mut written,
        )?;
        write_generated(
            project,
            &entity_folder.join("cl_init.lua"),
            render_client(project, bot, &material_relative, visual_available).into_bytes(),
            &mut generated,
            &mut written,
        )?;

        if matches!(bot.spawn_tab, SpawnTab::Custom) {
            custom_tabs
                .entry(bot.custom_tab_name.trim().to_owned())
                .or_default()
                .push(bot);
        }
    }

    let admin_only = project
        .nextbots
        .iter()
        .filter(|bot| bot.admin_only)
        .collect::<Vec<_>>();
    if !admin_only.is_empty() {
        let path = PathBuf::from("lua")
            .join("autorun")
            .join("server")
            .join(format!("nbc_{}_admin.lua", project.slug));
        write_generated(
            project,
            &path,
            render_admin_gate(project, &admin_only).into_bytes(),
            &mut generated,
            &mut written,
        )?;
    }

    if !sound_entries.is_empty() {
        let script_relative = PathBuf::from("lua")
            .join("autorun")
            .join(format!("nbc_{}_sounds.lua", project.slug));
        write_generated(
            project,
            &script_relative,
            format!("{}{}", watermark(), sound_entries).into_bytes(),
            &mut generated,
            &mut written,
        )?;
    }

    if !custom_tabs.is_empty() {
        let script = render_custom_tabs(project, &custom_tabs);
        let path = PathBuf::from("lua")
            .join("autorun")
            .join("client")
            .join(format!("nbc_{}_spawnmenu.lua", project.slug));
        write_generated(
            project,
            &path,
            script.into_bytes(),
            &mut generated,
            &mut written,
        )?;
    }

    let client_resources = generated
        .iter()
        .filter(|path| {
            path.extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| {
                    matches!(
                        extension.to_ascii_lowercase().as_str(),
                        "vmt" | "png" | "wav"
                    )
                })
        })
        .cloned()
        .collect::<Vec<_>>();
    if !client_resources.is_empty() {
        let path = PathBuf::from("lua")
            .join("autorun")
            .join("server")
            .join(format!("nbc_{}_resources.lua", project.slug));
        write_generated(
            project,
            &path,
            render_client_resources(&client_resources).into_bytes(),
            &mut generated,
            &mut written,
        )?;
    }

    let old_manifest = read_manifest(project);
    let mut removed = 0;
    for stale in old_manifest.files.difference(&generated) {
        if safe_relative(stale) {
            let path = project.root.join(stale);
            if path.is_file() && fs::remove_file(&path).is_ok() {
                removed += 1;
            }
        }
    }
    let manifest = GeneratedManifest {
        version: APP_VERSION.into(),
        files: generated,
    };
    let path = project.root.join(GENERATED_MANIFEST);
    fs::write(&path, serde_json::to_vec_pretty(&manifest)?)
        .map_err(|source| GenerationError::Io { path, source })?;

    Ok(GenerationReport {
        files_written: written,
        files_removed: removed,
        warnings,
    })
}

#[derive(Debug, Default)]
struct SoundNames {
    spawn: Option<String>,
    idle: Option<String>,
    damage: Option<String>,
    death: Option<String>,
    downed: Option<String>,
    footsteps: Option<String>,
    waves: BTreeMap<String, Vec<String>>,
}

fn convert_bot_audio(
    project: &Project,
    bot: &Nextbot,
    portable_root: &Path,
    generated: &mut BTreeSet<PathBuf>,
    written: &mut usize,
) -> Result<SoundNames, GenerationError> {
    let mut result = SoundNames::default();
    for (slot, sources) in [
        ("spawn", &bot.audio.spawn),
        ("idle", &bot.audio.idle),
        ("damage", &bot.audio.damage),
        ("death", &bot.audio.death),
        ("downed", &bot.audio.downed),
        ("footsteps", &bot.audio.footsteps),
    ] {
        if sources.is_empty() {
            continue;
        }
        let logical = format!("nbc.{}.{}.{}", project.slug, bot.class_name, slot);
        let mut waves = Vec::new();
        for (index, source) in sources.iter().enumerate() {
            let relative = PathBuf::from("sound")
                .join("nextbotcreator")
                .join(&project.slug)
                .join(&bot.class_name)
                .join(format!("{slot}_{:02}.wav", index + 1));
            let destination = project.root.join(&relative);
            converter::convert_audio(source, &destination, portable_root)?;
            generated.insert(relative.clone());
            *written += 1;
            waves.push(
                relative
                    .strip_prefix("sound")
                    .unwrap_or(&relative)
                    .to_string_lossy()
                    .replace('\\', "/"),
            );
        }
        match slot {
            "spawn" => result.spawn = Some(logical.clone()),
            "idle" => result.idle = Some(logical.clone()),
            "damage" => result.damage = Some(logical.clone()),
            "death" => result.death = Some(logical.clone()),
            "downed" => result.downed = Some(logical.clone()),
            "footsteps" => result.footsteps = Some(logical.clone()),
            _ => {}
        }
        result.waves.insert(logical, waves);
    }
    Ok(result)
}

fn render_sound_entries(bot: &Nextbot, sounds: &SoundNames) -> String {
    if sounds.waves.is_empty() {
        return String::new();
    }
    let mut output = String::new();
    for (logical, waves) in &sounds.waves {
        output.push_str(&format!(
            "sound.Add({{\n    name = {},\n    channel = CHAN_AUTO,\n    volume = {},\n    pitch = {},\n    level = {},\n    sound = ",
            lua_string(logical),
            bot.audio.volume.clamp(0.01, 1.0), bot.audio.pitch.clamp(1, 255), bot.audio.sound_level.clamp(20, 180)
        ));
        if waves.len() == 1 {
            output.push_str(&lua_string(&waves[0]));
        } else {
            output.push_str(&format!(
                "{{{}}}",
                waves
                    .iter()
                    .map(|wave| lua_string(wave))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        output.push_str("\n})\n\n");
    }
    output
}

fn render_shared(project: &Project, bot: &Nextbot, sounds: &SoundNames) -> String {
    let mut output = lua_header(project, bot);
    output.push_str("if not DrGBase then return end\n\n");
    output.push_str(&format!(
        "ENT.Base = {}\nENT.PrintName = {}\nENT.Category = {}\nENT.AdminOnly = {}\nENT.Spawnable = {}\n\n",
        lua_string(bot.base.lua_base()), lua_string(&bot.display_name), lua_string(&bot.category), bot.admin_only,
        matches!(bot.spawn_tab, SpawnTab::Npcs | SpawnTab::DrgBase)
    ));

    for section in crate::catalog::PropertySection::ALL {
        let fields = property_catalog()
            .into_iter()
            .filter(|field| field.section == section)
            .collect::<Vec<_>>();
        if fields.is_empty() {
            continue;
        }
        output.push_str(&format!("-- {} --\n", section.label()));
        for field in fields {
            if let Some(value) = bot.properties.get(field.name) {
                output.push_str(&format!("ENT.{} = {}\n", field.name, value.to_lua()));
            }
        }
        output.push('\n');
    }

    output.push_str(&format!(
        "ENT.OnSpawnSounds = {}\n",
        sound_list(&sounds.spawn)
    ));
    output.push_str(&format!(
        "ENT.OnIdleSounds = {}\n",
        sound_list(&sounds.idle)
    ));
    output.push_str(&format!(
        "ENT.OnDamageSounds = {}\n",
        sound_list(&sounds.damage)
    ));
    output.push_str(&format!(
        "ENT.OnDeathSounds = {}\n",
        sound_list(&sounds.death)
    ));
    output.push_str(&format!(
        "ENT.OnDownedSounds = {}\n",
        sound_list(&sounds.downed)
    ));
    output.push_str(&format!(
        "ENT.Footsteps = {}\n\n",
        sounds
            .footsteps
            .as_ref()
            .map(|name| format!("{{[MAT_DEFAULT] = {{{}}}}}", lua_string(name)))
            .unwrap_or_else(|| "{}".into())
    ));
    output.push_str(&render_possession(bot));
    output.push_str("\nDrGBase.AddNextbot(ENT)\n");
    output.push_str(&render_spawn_registration(bot));
    output
}

fn render_possession(bot: &Nextbot) -> String {
    let mut output = String::from("ENT.PossessionViews = {\n");
    for view in &bot.possession_views {
        output.push_str(&format!(
            "    {{offset = Vector({}, {}, {}), distance = {}, eyepos = {}}}, -- {}\n",
            format_number(view.offset[0] as f64),
            format_number(view.offset[1] as f64),
            format_number(view.offset[2] as f64),
            format_number(view.distance as f64),
            view.eye_position,
            view.name.replace(['\r', '\n'], " ")
        ));
    }
    output.push_str("}\nENT.PossessionBinds = {\n");
    for bind in &bot.possession_binds {
        let callback = match bind.trigger {
            BindTrigger::Pressed => "onkeypressed",
            BindTrigger::Held => "onkeydown",
            BindTrigger::Released => "onkeyreleased",
        };
        let (coroutine, action) = match bind.action {
            PossessionAction::PrimaryAttack => {
                (true, "self:OnMeleeAttack(self:PossessionGetLockedOn())")
            }
            PossessionAction::SecondaryAttack => {
                (true, "self:OnRangeAttack(self:PossessionGetLockedOn())")
            }
            PossessionAction::Reload => (true, "self:Reload()"),
            PossessionAction::Jump => (true, "self:Jump()"),
            PossessionAction::ToggleCrouch => (
                false,
                "if self.SetCrouching then self:SetCrouching(not self:IsCrouching()) end",
            ),
            PossessionAction::PlaySpawnSound => (
                false,
                "if #self.OnSpawnSounds > 0 then self:EmitSound(table.Random(self.OnSpawnSounds)) end",
            ),
        };
        output.push_str(&format!(
            "    [{}] = {{{{coroutine = {}, {} = function(self) {} end}}}},\n",
            bind.key, coroutine, callback, action
        ));
    }
    output.push_str("}\n");
    output
}

fn render_spawn_registration(bot: &Nextbot) -> String {
    let class = lua_string(&bot.class_name);
    let entry = format!(
        "{{Name = {}, Class = {}, Category = {}, AdminOnly = {}}}",
        lua_string(&bot.display_name),
        class,
        lua_string(&bot.category),
        bot.admin_only
    );
    match bot.spawn_tab {
        SpawnTab::Npcs => format!(
            "list.Set(\"NPC\", {class}, {entry})\nlist.Set(\"DrGBaseNextbots\", {class}, {entry})\n"
        ),
        SpawnTab::DrgBase => format!(
            "list.GetForEdit(\"NPC\")[{class}] = nil\nlist.Set(\"DrGBaseNextbots\", {class}, {entry})\n"
        ),
        SpawnTab::Entities => format!(
            "list.GetForEdit(\"NPC\")[{class}] = nil\nlist.GetForEdit(\"DrGBaseNextbots\")[{class}] = nil\nlist.Set(\"SpawnableEntities\", {class}, {{PrintName = {}, ClassName = {class}, Category = {}, AdminOnly = {}}})\n",
            lua_string(&bot.display_name),
            lua_string(&bot.category),
            bot.admin_only
        ),
        SpawnTab::Custom => format!(
            "list.GetForEdit(\"NPC\")[{class}] = nil\nlist.GetForEdit(\"DrGBaseNextbots\")[{class}] = nil\n"
        ),
    }
}

fn render_server(bot: &Nextbot) -> String {
    let mut output = format!(
        "{}AddCSLuaFile(\"cl_init.lua\")\nAddCSLuaFile(\"shared.lua\")\ninclude(\"shared.lua\")\n\n",
        watermark()
    );
    let uses_melee_helper = bot.combat.melee_enabled
        || bot
            .hook_recipes
            .iter()
            .flat_map(|recipe| &recipe.actions)
            .any(|action| action.kind == HookActionKind::PerformMeleeAttack);
    let uses_range_helper = bot.combat.ranged_enabled
        || bot
            .hook_recipes
            .iter()
            .flat_map(|recipe| &recipe.actions)
            .any(|action| action.kind == HookActionKind::PerformRangeAttack);
    if uses_melee_helper {
        output.push_str(&format!(
            "function ENT:NBCPerformMeleeAttack()\n    self:Attack({{\n        damage = math.Rand({}, {}),\n        delay = {},\n        type = {},\n        range = self.MeleeAttackRange\n    }})\nend\n\n",
            format_number(bot.combat.melee_damage_min as f64), format_number(bot.combat.melee_damage_max as f64),
            format_number(bot.combat.melee_delay as f64), bot.combat.melee_damage_type
        ));
    }
    if uses_range_helper {
        output.push_str(&format!(
            "function ENT:NBCPerformRangeAttack()\n    local projectile = self:CreateProjectile({}, {{\n        Contact = function(entity, target)\n            if not IsValid(target) or target == self then return end\n            target:TakeDamage({}, self, entity)\n            entity:Remove()\n        end\n    }})\n    if IsValid(projectile) then\n        projectile:SetPos(self:EyePos() + self:GetForward()*20)\n        self:AimProjectile(projectile, {})\n    end\nend\n\n",
            lua_string(&bot.combat.projectile_class), format_number(bot.combat.ranged_damage as f64),
            format_number(bot.combat.ranged_speed as f64)
        ));
    }

    let mut bodies: BTreeMap<HookEvent, Vec<String>> = BTreeMap::new();
    if bot.hooks.patrol_when_idle {
        bodies.entry(HookEvent::OnIdle).or_default().push(format!(
            "self:AddPatrolPos(self:RandomPos({}))",
            format_number(bot.hooks.patrol_radius as f64)
        ));
        bodies
            .entry(HookEvent::OnReachedPatrol)
            .or_default()
            .push(format!(
                "self:Wait(math.Rand({}, {}))",
                format_number(bot.hooks.patrol_wait_min as f64),
                format_number(bot.hooks.patrol_wait_max as f64)
            ));
    }
    if bot.hooks.spot_damage_attacker {
        bodies.entry(HookEvent::OnTakeDamage).or_default().extend([
            "local attacker = damage:GetAttacker()".into(),
            "if IsValid(attacker) then self:SpotEntity(attacker) end".into(),
        ]);
    }
    if bot.combat.melee_enabled {
        bodies.entry(HookEvent::OnMeleeAttack).or_default().extend([
            "self:NBCPerformMeleeAttack()".into(),
            format!(
                "self:PlayAnimationAndMove({}, 1, self.FaceEnemy)",
                bot.combat.melee_animation
            ),
        ]);
    }
    if bot.combat.ranged_enabled {
        bodies.entry(HookEvent::OnRangeAttack).or_default().extend([
            "self:NBCPerformRangeAttack()".into(),
            format!(
                "self:PlayAnimationAndMove({}, 1, self.FaceEnemy)",
                bot.combat.ranged_animation
            ),
            format!(
                "self:Wait({})",
                format_number(bot.combat.ranged_cooldown as f64)
            ),
        ]);
    }
    if bot.hooks.remove_on_death {
        bodies
            .entry(HookEvent::OnDeath)
            .or_default()
            .push("self:Remove()".into());
    }
    for recipe in &bot.hook_recipes {
        if !recipe.event.is_client() {
            bodies
                .entry(recipe.event)
                .or_default()
                .extend(render_hook_actions(recipe.event, &recipe.actions, false));
        }
    }
    for (event, mut lines) in bodies {
        if event == HookEvent::OnPatrolling {
            lines.push("return self:WhilePatrolling(position, patrol)".into());
        }
        output.push_str(&format!(
            "function ENT:{}({})\n",
            event.lua_name(),
            event.lua_parameters()
        ));
        for line in lines {
            output.push_str(&indent_lua(&line, 1));
        }
        output.push_str("end\n\n");
    }
    output
}

fn render_client(
    project: &Project,
    bot: &Nextbot,
    material_relative: &str,
    visual_available: bool,
) -> String {
    let mut output = format!("{}include(\"shared.lua\")\n", watermark());
    if visual_available && matches!(bot.base, crate::domain::BaseVariant::Sprite) {
        output.push_str(&format!(
            "\nlocal spriteMaterial = Material({})\n\nfunction ENT:DrawTranslucent()\n    if not self:ShouldDraw() then return end\n    local position = self:GetPos() + Vector(0, 0, {} + {}/2)\n    render.SetMaterial(spriteMaterial)\n    render.DrawSprite(position, {}, {}, self:GetColor())\n    self:_BaseDraw()\n    self:CustomDraw()\n    self:_DrawDebug()\n    if self:IsPossessedByLocalPlayer() then self:PossessionDraw() end\nend\n",
            lua_string(material_relative), format_number(bot.visual.vertical_offset as f64), format_number(bot.visual.height as f64),
            format_number(bot.visual.width as f64), format_number(bot.visual.height as f64)
        ));
    }
    let mut bodies: BTreeMap<HookEvent, Vec<String>> = BTreeMap::new();
    if visual_available && !matches!(bot.base, crate::domain::BaseVariant::Sprite) {
        bodies
            .entry(HookEvent::ClientInitialize)
            .or_default()
            .push(format!(
                "self:SetMaterial({})",
                lua_string(material_relative)
            ));
    }
    for recipe in &bot.hook_recipes {
        if recipe.event.is_client() {
            bodies
                .entry(recipe.event)
                .or_default()
                .extend(render_hook_actions(recipe.event, &recipe.actions, true));
        }
    }
    for (event, lines) in bodies {
        if lines.is_empty() {
            continue;
        }
        output.push_str(&format!(
            "\nfunction ENT:{}({})\n",
            event.lua_name(),
            event.lua_parameters()
        ));
        for line in lines {
            output.push_str(&indent_lua(&line, 1));
        }
        output.push_str("end\n");
    }
    let _ = project;
    output
}

fn render_hook_actions(
    event: HookEvent,
    actions: &[crate::domain::HookAction],
    client: bool,
) -> Vec<String> {
    let related = event.related_entity();
    actions
        .iter()
        .filter_map(|action| {
            let sound = |property: &str| {
                format!(
                    "if #self.{property} > 0 then self:EmitSound(table.Random(self.{property})) end"
                )
            };
            match action.kind {
                HookActionKind::PlaySpawnSound => Some(sound("OnSpawnSounds")),
                HookActionKind::PlayIdleSound => Some(sound("OnIdleSounds")),
                HookActionKind::PlayDamageSound => Some(sound("OnDamageSounds")),
                _ if client => None,
                HookActionKind::Wait => Some(format!(
                    "self:CallInCoroutine(function(self)\n    self:Wait({})\nend)",
                    format_number(action.value.max(0.0) as f64)
                )),
                HookActionKind::AddRandomPatrol => Some(format!(
                    "self:AddPatrolPos(self:RandomPos({}))",
                    format_number(action.value.max(0.0) as f64)
                )),
                HookActionKind::SpotRelatedEntity => Some(format!(
                    "if IsValid({related}) then self:SpotEntity({related}) end"
                )),
                HookActionKind::SetEnemyToRelated => Some(format!(
                    "if IsValid({related}) then self:SetEnemy({related}) end"
                )),
                HookActionKind::ClearEnemy => Some("self:SetEnemy(NULL)".into()),
                HookActionKind::Heal => Some(format!(
                    "self:AddHealth({})",
                    format_number(action.value as f64)
                )),
                HookActionKind::DisableAi => Some("self:DisableAI()".into()),
                HookActionKind::EnableAi => Some("self:EnableAI()".into()),
                HookActionKind::PerformMeleeAttack => Some("self:NBCPerformMeleeAttack()".into()),
                HookActionKind::PerformRangeAttack => Some("self:NBCPerformRangeAttack()".into()),
                HookActionKind::RemoveSelf => Some("self:Remove()".into()),
            }
        })
        .collect()
}

fn indent_lua(text: &str, levels: usize) -> String {
    let prefix = "    ".repeat(levels);
    text.lines()
        .map(|line| format!("{prefix}{line}\n"))
        .collect()
}

fn render_custom_tabs(project: &Project, tabs: &BTreeMap<String, Vec<&Nextbot>>) -> String {
    let mut output = format!("{}if not spawnmenu then return end\n\n", watermark());
    for (index, (tab, bots)) in tabs.iter().enumerate() {
        output.push_str(&format!(
            "spawnmenu.AddCreationTab({}, function()\n    local panel = vgui.Create(\"ContentContainer\")\n",
            lua_string(tab)
        ));
        for bot in bots {
            output.push_str(&format!(
                "    spawnmenu.CreateContentIcon(\"npc\", panel, {{nicename = {}, spawnname = {}, material = {}, admin = {}}})\n",
                lua_string(&bot.display_name), lua_string(&bot.class_name), lua_string(&format!("entities/{}.png", bot.class_name)), bot.admin_only
            ));
        }
        output.push_str(&format!(
            "    return panel\nend, \"icon16/monkey.png\", {})\n\n",
            30 + index
        ));
    }
    output.push_str(&format!("-- Project: {}\n", one_line(&project.name)));
    output
}

fn render_admin_gate(project: &Project, bots: &[&Nextbot]) -> String {
    let mut output = format!(
        "{}if not SERVER then return end\n\nlocal adminOnly = {{\n",
        watermark()
    );
    for bot in bots {
        output.push_str(&format!("    [{}] = true,\n", lua_string(&bot.class_name)));
    }
    output.push_str(&format!(
        "}}\n\nlocal function canSpawn(ply, class)\n    if adminOnly[class] and IsValid(ply) and not ply:IsAdmin() then\n        return false\n    end\nend\n\nhook.Add(\"PlayerSpawnNPC\", {}, canSpawn)\nhook.Add(\"PlayerSpawnSENT\", {}, canSpawn)\n",
        lua_string(&format!("NextbotCreator.AdminOnlyNPC.{}", project.slug)),
        lua_string(&format!("NextbotCreator.AdminOnlySENT.{}", project.slug))
    ));
    output
}

fn render_client_resources(paths: &[PathBuf]) -> String {
    let mut output = format!("{}if not SERVER then return end\n\n", watermark());
    for path in paths {
        output.push_str(&format!(
            "resource.AddFile({})\n",
            lua_string(&path.to_string_lossy().replace('\\', "/"))
        ));
    }
    output
}

fn lua_header(project: &Project, bot: &Nextbot) -> String {
    format!(
        "{}-- Project: {}\n-- Entity: {}\n-- DRGBase baseline: {}\n\n",
        watermark(),
        one_line(&project.name),
        bot.class_name,
        DRGBASE_BASELINE_COMMIT
    )
}

fn watermark() -> String {
    format!("-- This nextbot was created by NextbotCreator {APP_VERSION}\n")
}

fn one_line(value: &str) -> String {
    value.replace(['\r', '\n'], " ")
}

fn sound_list(value: &Option<String>) -> String {
    value
        .as_ref()
        .map(|value| format!("{{{}}}", lua_string(value)))
        .unwrap_or_else(|| "{}".into())
}

fn write_generated(
    project: &Project,
    relative: &Path,
    bytes: Vec<u8>,
    generated: &mut BTreeSet<PathBuf>,
    written: &mut usize,
) -> Result<(), GenerationError> {
    if !safe_relative(relative) {
        return Err(GenerationError::Validation(format!(
            "Unsafe generated path: {}",
            relative.display()
        )));
    }
    let path = project.root.join(relative);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| GenerationError::Io {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    fs::write(&path, bytes).map_err(|source| GenerationError::Io {
        path: path.clone(),
        source,
    })?;
    generated.insert(relative.to_path_buf());
    *written += 1;
    Ok(())
}

fn safe_relative(path: &Path) -> bool {
    !path.is_absolute()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

fn read_manifest(project: &Project) -> GeneratedManifest {
    let path = project.root.join(GENERATED_MANIFEST);
    fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_lua_is_watermarked_and_registers_admin_npcs() {
        let root = std::env::temp_dir().join("nbc_generator_test");
        let mut project = Project::new("Example", root);
        project.nextbots[0].admin_only = true;
        let shared = render_shared(&project, &project.nextbots[0], &SoundNames::default());
        assert!(shared.starts_with("-- This nextbot was created by NextbotCreator 0.1.0"));
        assert!(shared.contains("AdminOnly = true"));
        assert!(shared.contains("ENT.Base = \"drgbase_nextbot_sprite\""));
        assert!(shared.contains("DrGBase.AddNextbot(ENT)"));
        let gate = render_admin_gate(&project, &[&project.nextbots[0]]);
        assert!(gate.contains("PlayerSpawnNPC"));
        assert!(gate.contains("PlayerSpawnSENT"));
        assert!(gate.contains("not ply:IsAdmin()"));
    }

    #[test]
    fn all_documented_properties_are_emitted() {
        let root = std::env::temp_dir().join("nbc_generator_catalog_test");
        let project = Project::new("Example", root);
        let shared = render_shared(&project, &project.nextbots[0], &SoundNames::default());
        for property in property_catalog() {
            assert!(
                shared.contains(&format!("ENT.{} =", property.name)),
                "missing {}",
                property.name
            );
        }
    }

    #[test]
    fn parent_paths_are_rejected() {
        assert!(!safe_relative(Path::new("../outside")));
        assert!(safe_relative(Path::new("lua/entities/example/shared.lua")));
    }

    #[test]
    fn default_project_validates_and_generates_three_file_entity() {
        let root =
            std::env::temp_dir().join(format!("nbc_generator_integration_{}", std::process::id()));
        if root.exists() {
            fs::remove_dir_all(&root).unwrap();
        }
        let mut project = Project::new("Example", root.clone());
        project.nextbots[0]
            .hook_recipes
            .push(crate::domain::HookRecipe {
                event: HookEvent::OnSpawn,
                actions: vec![crate::domain::HookAction {
                    kind: HookActionKind::Wait,
                    value: 0.5,
                }],
            });
        assert!(validate_project(&project).is_ok());
        let report = generate_project(&project, &root).unwrap();
        assert!(report.files_written >= 4);
        for file in ["shared.lua", "init.lua", "cl_init.lua"] {
            let path = root.join("lua/entities/npc_my_nextbot").join(file);
            let lua = fs::read_to_string(path).unwrap();
            assert!(lua.starts_with("-- This nextbot was created by NextbotCreator 0.1.0"));
        }
        let server = fs::read_to_string(root.join("lua/entities/npc_my_nextbot/init.lua")).unwrap();
        assert!(server.contains("function ENT:OnSpawn()"));
        assert!(server.contains("self:CallInCoroutine(function(self)"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn unsafe_raw_constants_are_rejected() {
        let root = std::env::temp_dir().join("nbc_generator_validation_test");
        let mut project = Project::new("Example", root);
        project.nextbots[0].combat.melee_damage_type = "DMG_SLASH; RunString('x')".into();
        assert!(matches!(
            validate_project(&project),
            Err(GenerationError::Validation(_))
        ));
    }
}
