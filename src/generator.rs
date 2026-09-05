use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::catalog::{DRGBASE_BASELINE_COMMIT, property_catalog};
use crate::converter::{self, ConversionError};
use crate::domain::{
    ATTACK_ACTIVITIES, AudioSlot, BindTrigger, DAMAGE_TYPES, HookActionKind, HookEvent,
    KillfeedIconMode, Nextbot, POSSESSION_KEYS, PossessionAction, Project, PropertyValue, SpawnTab,
    format_number, lua_string, sanitize_class_name, slugify,
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
        for slot in AudioSlot::ALL {
            for clip in slot.get(&bot.audio) {
                if let Err(error) = clip.trim.validate() {
                    errors.push(format!("{} / {}: {error}", bot.display_name, slot.label()));
                }
            }
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
        if matches!(bot.visual.killfeed_icon.mode, KillfeedIconMode::CustomImage)
            && bot.visual.killfeed_icon.source.is_none()
        {
            warnings.push(format!(
                "{} uses a custom killfeed icon but no image is selected.",
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

        let killfeed_source = match bot.visual.killfeed_icon.mode {
            KillfeedIconMode::NextbotSprite => bot.visual.source.as_ref(),
            KillfeedIconMode::CustomImage => bot.visual.killfeed_icon.source.as_ref(),
        };
        let mut killfeed_material = None;
        if let Some(source) = killfeed_source {
            if source.is_file() {
                let material = format!("{material_relative}_killfeed");
                let artifact = converter::convert_killfeed_icon(
                    source,
                    &project.root.join("materials"),
                    &material,
                )?;
                for path in [&artifact.vtf_path, &artifact.vmt_path] {
                    if let Ok(relative) = path.strip_prefix(&project.root) {
                        generated.insert(relative.to_path_buf());
                    }
                }
                written += 2;
                killfeed_material = Some(material);
            } else {
                warnings.push(format!(
                    "{}: killfeed icon source is missing: {}",
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
            render_shared(project, bot, &sound_names, killfeed_material.as_deref()).into_bytes(),
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
                        "vtf" | "vmt" | "png" | "wav"
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
    slots: BTreeMap<AudioSlot, String>,
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
    for slot in AudioSlot::ALL {
        let sources = slot.get(&bot.audio);
        let key = slot.key();
        if sources.is_empty() {
            continue;
        }
        let logical = format!("nbc.{}.{}.{}", project.slug, bot.class_name, key);
        let mut waves = Vec::new();
        for (index, source) in sources.iter().enumerate() {
            let relative = PathBuf::from("sound")
                .join("nextbotcreator")
                .join(&project.slug)
                .join(&bot.class_name)
                .join(format!("{key}_{:02}.wav", index + 1));
            let destination = project.root.join(&relative);
            converter::convert_audio_clip(source, &destination, portable_root)?;
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
        result.slots.insert(slot, logical.clone());
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

fn render_shared(
    project: &Project,
    bot: &Nextbot,
    sounds: &SoundNames,
    killfeed_material: Option<&str>,
) -> String {
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
                let value = if field.name == "IdleSoundDelay" && bot.audio.idle_loop {
                    "0".to_owned()
                } else {
                    value.to_lua()
                };
                output.push_str(&format!("ENT.{} = {}\n", field.name, value));
            }
        }
        output.push('\n');
    }

    for slot in AudioSlot::ALL {
        let name = sounds.slots.get(&slot).cloned();
        let value = if slot == AudioSlot::Footsteps {
            name.as_ref()
                .map(|name| format!("{{[MAT_DEFAULT] = {{{}}}}}", lua_string(name)))
                .unwrap_or_else(|| "{}".into())
        } else {
            sound_list(&name)
        };
        output.push_str(&format!("ENT.{} = {}\n", slot.lua_field(), value));
    }
    output.push_str(&format!(
        "ENT.NBCIgnoreNextbots = {}\n",
        bot.ignore_nextbots
    ));
    if let Some(name) = sounds.slots.get(&AudioSlot::Chase) {
        let clips = sounds.waves.get(name).cloned().unwrap_or_default();
        output.push_str(&format!(
            "ENT.NBCChaseClips = {}\nENT.NBCSoundVolume = {}\nENT.NBCSoundPitch = {}\nENT.NBCSoundLevel = {}\n",
            PropertyValue::StringList(clips).to_lua(),
            bot.audio.volume.clamp(0.01, 1.0), bot.audio.pitch.clamp(1, 255), bot.audio.sound_level.clamp(20, 180)
        ));
    }
    if let Some(material) = killfeed_material {
        output.push_str(&format!(
            "if CLIENT then\n    ENT.Killicon = {{icon = {}, color = Color(255, 255, 255, 255)}}\nend\n\n",
            lua_string(material)
        ));
    }
    output.push_str(&render_possession(bot));
    if sounds.slots.contains_key(&AudioSlot::Jump) {
        output.push_str(
            "\nif SERVER then\n    function ENT:OnLeaveGround()\n        if #self.JumpSounds > 0 then\n            self:EmitSound(table.Random(self.JumpSounds))\n        end\n    end\nend\n",
        );
    }
    if sounds.slots.contains_key(&AudioSlot::Land) {
        output.push_str("\nif SERVER then\n    function ENT:OnLandOnGround()\n        if #self.NBCLandSounds > 0 then\n            self:EmitSound(table.Random(self.NBCLandSounds))\n        end\n    end\nend\n");
    }
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
    if bot.ignore_nextbots {
        output.push_str("function ENT:ShouldIgnore(entity)\n    return IsValid(entity) and entity:IsNextBot()\nend\n\n");
    }
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
            "function ENT:NBCPerformMeleeAttack()\n    if #self.NBCMeleeSounds > 0 then self:EmitSound(table.Random(self.NBCMeleeSounds)) end\n    self:Attack({{\n        damage = function(target)\n            if self.NBCIgnoreNextbots and target:IsNextBot() then return 0 end\n            return math.Rand({}, {})\n        end,\n        delay = {},\n        type = {},\n        range = self.MeleeAttackRange\n    }})\nend\n\n",
            format_number(bot.combat.melee_damage_min as f64), format_number(bot.combat.melee_damage_max as f64),
            format_number(bot.combat.melee_delay as f64), bot.combat.melee_damage_type
        ));
    }
    if uses_range_helper {
        output.push_str(&format!(
            "function ENT:NBCPerformRangeAttack()\n    if #self.NBCRangedSounds > 0 then self:EmitSound(table.Random(self.NBCRangedSounds)) end\n    local projectile = self:CreateProjectile({}, {{\n        Contact = function(entity, target)\n            if not IsValid(target) or target == self then return end\n            if self.NBCIgnoreNextbots and target:IsNextBot() then return end\n            target:TakeDamage({}, self, entity)\n            entity:Remove()\n        end\n    }})\n    if IsValid(projectile) then\n        projectile:SetPos(self:EyePos() + self:GetForward()*20)\n        self:AimProjectile(projectile, {})\n    end\nend\n\n",
            lua_string(&bot.combat.projectile_class), format_number(bot.combat.ranged_damage as f64),
            format_number(bot.combat.ranged_speed as f64)
        ));
    }
    let mut bodies: BTreeMap<HookEvent, Vec<String>> = BTreeMap::new();
    if bot.ignore_nextbots {
        bodies.entry(HookEvent::OnTakeDamage).or_default().push(
            "if IsValid(damage:GetAttacker()) and damage:GetAttacker():IsNextBot() then return true end".into()
        );
    }
    for (slot, event) in [
        (AudioSlot::Alert, HookEvent::OnNewEnemy),
        (AudioSlot::LostEnemy, HookEvent::OnLastEnemy),
    ] {
        if !slot.get(&bot.audio).is_empty() {
            bodies.entry(event).or_default().push(format!(
                "if not self:IsDead() and not self:IsDown() and #self.{0} > 0 then self:EmitSound(table.Random(self.{0})) end", slot.lua_field()
            ));
        }
    }
    if !bot.audio.chase.is_empty() {
        // Keep playback separate from user CustomThink recipes, which can choose their own delay.
        bodies.entry(HookEvent::ServerInitialize).or_default().push(
            r#"local chaseTimer = "NBCChaseSound_"..self:GetCreationID()
self:CallOnRemove("NBCChaseSound", function(entity)
    timer.Remove(chaseTimer)
    if entity.NBCPlayingChase then entity:StopSound(entity.NBCPlayingChase) end
end)
timer.Create(chaseTimer, 0.1, 0, function()
    if not IsValid(self) then timer.Remove(chaseTimer) return end
    local pursuing = self:HasEnemy() and not self:IsDead() and not self:IsDown()
        and not self:IsAIDisabled() and not self:IsPossessed()
    if not pursuing then
        if self.NBCPlayingChase then self:StopSound(self.NBCPlayingChase) end
        self.NBCPlayingChase = nil
        self.NBCNextChaseSound = 0
    elseif CurTime() >= (self.NBCNextChaseSound or 0) and #self.NBCChaseClips > 0 then
        if self.NBCPlayingChase then self:StopSound(self.NBCPlayingChase) end
        local clip = table.Random(self.NBCChaseClips)
        self:EmitSound(clip, self.NBCSoundLevel, self.NBCSoundPitch, self.NBCSoundVolume)
        self.NBCPlayingChase = clip
        self.NBCNextChaseSound = CurTime() + math.max(SoundDuration(clip)*100/self.NBCSoundPitch, 0.1)
    end
end)"#
                .into(),
        );
        for event in [
            HookEvent::OnLastEnemy,
            HookEvent::OnDeath,
            HookEvent::OnDowned,
        ] {
            bodies.entry(event).or_default().insert(0,
                "if self.NBCPlayingChase then self:StopSound(self.NBCPlayingChase) end\nself.NBCPlayingChase = nil\nself.NBCNextChaseSound = 0".into()
            );
        }
    }
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
        let shared = render_shared(&project, &project.nextbots[0], &SoundNames::default(), None);
        assert!(shared.starts_with(&format!(
            "-- This nextbot was created by NextbotCreator {APP_VERSION}"
        )));
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
        let shared = render_shared(&project, &project.nextbots[0], &SoundNames::default(), None);
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
            assert!(lua.starts_with(&format!(
                "-- This nextbot was created by NextbotCreator {APP_VERSION}"
            )));
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

    #[test]
    fn killfeed_material_and_jump_sound_hooks_are_emitted() {
        let root = std::env::temp_dir().join("nbc_generator_media_test");
        let project = Project::new("Example", root);
        let mut bot = project.nextbots[0].clone();
        bot.audio.jump.push(PathBuf::from("jump.mp3").into());
        let sounds = SoundNames {
            slots: BTreeMap::from([(AudioSlot::Jump, "nbc.example.npc_my_nextbot.jump".into())]),
            ..SoundNames::default()
        };
        let shared = render_shared(
            &project,
            &bot,
            &sounds,
            Some("nextbotcreator/example/npc_my_nextbot/killfeed"),
        );
        assert!(shared.contains("ENT.JumpSounds = {\"nbc.example.npc_my_nextbot.jump\"}"));
        assert!(
            shared.contains(
                "ENT.Killicon = {icon = \"nextbotcreator/example/npc_my_nextbot/killfeed\""
            )
        );
        assert!(shared.contains("function ENT:OnLeaveGround()"));
        assert!(shared.contains("table.Random(self.JumpSounds)"));
        assert!(
            shared.find("function ENT:OnLeaveGround()").unwrap()
                < shared.find("DrGBase.AddNextbot(ENT)").unwrap()
        );
    }

    #[test]
    fn idle_loop_removes_the_delay_between_idle_sounds() {
        let root = std::env::temp_dir().join("nbc_generator_idle_loop_test");
        let project = Project::new("Example", root);
        let mut bot = project.nextbots[0].clone();
        bot.audio.idle_loop = true;
        let shared = render_shared(&project, &bot, &SoundNames::default(), None);
        assert!(shared.contains("ENT.IdleSoundDelay = 0"));
        assert!(!shared.contains("ENT.IdleSoundDelay = 2"));
    }

    #[test]
    fn nextbot_protection_precedes_damage_recipes_and_can_be_disabled() {
        use crate::domain::{HookAction, HookRecipe};
        let mut bot = Nextbot::new("Hunter", "npc_hunter");
        bot.combat.ranged_enabled = true;
        bot.hook_recipes.push(HookRecipe {
            event: HookEvent::OnTakeDamage,
            actions: vec![HookAction {
                kind: HookActionKind::Heal,
                value: 5.0,
            }],
        });
        let server = render_server(&bot);
        assert!(server.contains("function ENT:ShouldIgnore(entity)"));
        assert!(server.contains("target:IsNextBot() then return 0 end"));
        assert!(server.contains("target:IsNextBot() then return end"));
        let guard = server
            .find("damage:GetAttacker():IsNextBot() then return true end")
            .unwrap();
        assert!(
            guard
                < server
                    .find("local attacker = damage:GetAttacker()")
                    .unwrap()
        );
        assert_eq!(server.matches("function ENT:OnTakeDamage(").count(), 1);
        bot.ignore_nextbots = false;
        let server = render_server(&bot);
        assert!(!server.contains("function ENT:ShouldIgnore"));
        assert!(!server.contains("damage:GetAttacker():IsNextBot() then return true end"));
    }

    #[test]
    fn sound_hooks_merge_with_recipes_and_chase_cleans_up() {
        use crate::domain::{HookAction, HookRecipe};
        let mut project = Project::new("Sounds", PathBuf::from("sounds"));
        let bot = &mut project.nextbots[0];
        for slot in AudioSlot::ALL {
            slot.get_mut(&mut bot.audio)
                .push(PathBuf::from("clip.wav").into());
        }
        bot.hook_recipes.push(HookRecipe {
            event: HookEvent::ServerInitialize,
            actions: vec![HookAction {
                kind: HookActionKind::PlaySpawnSound,
                value: 0.0,
            }],
        });
        bot.hook_recipes.push(HookRecipe {
            event: HookEvent::OnNewEnemy,
            actions: vec![HookAction {
                kind: HookActionKind::PlayDamageSound,
                value: 0.0,
            }],
        });
        let server = render_server(bot);
        assert_eq!(server.matches("function ENT:CustomInitialize(").count(), 1);
        assert_eq!(server.matches("function ENT:OnNewEnemy(").count(), 1);
        assert!(server.contains("table.Random(self.NBCAlertSounds)"));
        assert!(server.contains("table.Random(self.OnDamageSounds)"));
        assert!(server.contains("table.Random(self.OnSpawnSounds)"));
        assert!(server.contains("timer.Create(chaseTimer, 0.1, 0, function()"));
        assert!(server.contains("timer.Remove(chaseTimer)"));
        assert!(server.contains("SoundDuration(clip)*100/self.NBCSoundPitch"));
        for hook in ["OnLastEnemy", "OnDeath", "OnDowned"] {
            let body = server
                .split(&format!("function ENT:{hook}("))
                .nth(1)
                .unwrap()
                .split("\nend")
                .next()
                .unwrap();
            assert!(
                body.contains("self:StopSound(self.NBCPlayingChase)"),
                "{hook}"
            );
        }
    }

    #[test]
    #[ignore = "functional smoke test: requires NEXTBOTCREATOR_FFMPEG or FFmpeg on PATH"]
    fn all_sound_slots_convert_register_and_remove_only_manifest_files() {
        let root = std::env::temp_dir().join(format!("nbc_sound_smoke_{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let mut project = Project::new("Sound smoke", root.clone());
        let source = root.join("source.wav");
        let samples = vec![0_u8; 8820];
        let mut wav = Vec::new();
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(36 + samples.len() as u32).to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        wav.extend_from_slice(&16_u32.to_le_bytes());
        wav.extend_from_slice(&1_u16.to_le_bytes());
        wav.extend_from_slice(&1_u16.to_le_bytes());
        wav.extend_from_slice(&44100_u32.to_le_bytes());
        wav.extend_from_slice(&88200_u32.to_le_bytes());
        wav.extend_from_slice(&2_u16.to_le_bytes());
        wav.extend_from_slice(&16_u16.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&(samples.len() as u32).to_le_bytes());
        wav.extend_from_slice(&samples);
        fs::write(&source, wav).unwrap();
        for slot in AudioSlot::ALL {
            slot.get_mut(&mut project.nextbots[0].audio)
                .extend([source.clone().into(), source.clone().into()]);
        }
        crate::persistence::save_project(&project).unwrap();
        let mut project = crate::persistence::load_project(&root).unwrap();
        generate_project(&project, &root).unwrap();
        let shared =
            fs::read_to_string(root.join("lua/entities/npc_my_nextbot/shared.lua")).unwrap();
        let sound_script =
            fs::read_to_string(root.join("lua/autorun/nbc_sound_smoke_sounds.lua")).unwrap();
        assert!(shared.contains("function ENT:OnLandOnGround()"));
        assert!(shared.contains("ENT.NBCChaseClips = {\"nextbotcreator/"));
        assert!(sound_script.starts_with(&watermark()));
        for slot in AudioSlot::ALL {
            assert!(shared.contains(&format!("ENT.{} = ", slot.lua_field())));
            assert!(
                sound_script.contains(&format!("nbc.sound_smoke.npc_my_nextbot.{}", slot.key()))
            );
            for index in 1..=2 {
                let wave = root.join(format!(
                    "sound/nextbotcreator/sound_smoke/npc_my_nextbot/{}_{index:02}.wav",
                    slot.key()
                ));
                let bytes = fs::read(wave).unwrap();
                assert_eq!(&bytes[..4], b"RIFF");
                assert!(bytes.windows(4).any(|chunk| chunk == b"data"));
            }
        }
        let untracked = root.join("sound/user-recording.wav");
        fs::write(&untracked, b"keep me").unwrap();
        for slot in AudioSlot::ALL {
            slot.get_mut(&mut project.nextbots[0].audio).clear();
        }
        let report = generate_project(&project, &root).unwrap();
        assert!(report.files_removed >= 27);
        assert!(source.is_file());
        assert_eq!(fs::read(untracked).unwrap(), b"keep me");
        fs::remove_dir_all(root).unwrap();
    }
}
