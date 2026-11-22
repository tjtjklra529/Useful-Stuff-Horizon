use super::*;
fn colored(value: impl Into<String>) -> Text {
    let s = value.into();
    let mut out = Text::literal(if s.starts_with('§') {
        s[3..].to_owned()
    } else {
        s.clone()
    });
    out.style.italic = Some(false);
    out.style.color = Some(
        match s.chars().nth(1).unwrap_or('7') {
            'a' => "green",
            'b' => "aqua",
            'd' => "light_purple",
            'e' => "yellow",
            '6' => "gold",
            '2' => "dark_green",
            '5' => "dark_purple",
            '8' => "dark_gray",
            'f' => "white",
            _ => "gray",
        }
        .to_owned(),
    );
    out
}

pub fn accepts(h: &dyn Host, s: &ItemStack, flint: bool) -> bool {
    let name = id(h, s);
    if flint {
        matches!(
            name.as_str(),
            "minecraft:flint" | "useful_stuff:custom_shot"
        )
    } else {
        matches!(
            name.as_str(),
            "minecraft:arrow"
                | "minecraft:tipped_arrow"
                | "minecraft:spectral_arrow"
                | "useful_stuff:custom_arrow"
        )
    }
}
pub fn abilities(s: &ItemStack) -> Vec<String> {
    match field(&data(s), "Abilities") {
        Some(Tag::List(v)) => v
            .iter()
            .filter_map(|t| {
                if let Tag::String(s) = t {
                    Some(s.to_string_lossy())
                } else {
                    None
                }
            })
            .collect(),
        _ => vec![],
    }
}
pub fn has(s: &ItemStack, name: &str) -> bool {
    abilities(s).iter().any(|s| s == name)
}
fn number(s: &ItemStack, name: &str, fallback: f64) -> f64 {
    match field(&data(s), name) {
        Some(Tag::Double(n)) => f64::from_bits(*n),
        _ => fallback,
    }
}
pub fn damage(s: &ItemStack) -> f32 {
    (number(s, "Damage", 4.) + if has(s, "weight") { 1. } else { 0. }).clamp(1., 10.) as f32
}
pub fn speed(_: &dyn Host, s: &ItemStack) -> f64 {
    (number(s, "Speed", 1.)
        * if has(s, "swift") { 1.15 } else { 1. }
        * if has(s, "weight") { 0.85 } else { 1. })
    .clamp(0.5, 1.5)
}
pub fn gravity(s: &ItemStack) -> f64 {
    (number(s, "Gravity", 1.) * if has(s, "weight") { 1.2 } else { 1. }).clamp(0.5, 2.)
}
pub fn grasp(s: &ItemStack) -> i32 {
    match component(s, "minecraft:enchantments") {
        Some(t) => match t
            .get("useful_stuff:grasping")
            .or_else(|| t.get("levels").and_then(|l| l.get("useful_stuff:grasping")))
        {
            Some(Tag::Int(n)) => (*n).clamp(0, 4),
            _ => integer(s, "Grasping").clamp(0, 4),
        },
        _ => integer(s, "Grasping").clamp(0, 4),
    }
}
pub fn core(h: &dyn Host, s: &ItemStack) -> bool {
    is(h, s, "custom_shot") || profiles::profile(&id(h, s)).is_some_and(|p| p.kind == "core")
}
pub fn modifier(h: &dyn Host, s: &ItemStack) -> bool {
    let name = id(h, s);
    profiles::profile(&name).is_some_and(|p| p.kind == "modifier")
        || name.ends_with("_dye")
        || matches!(
            name.as_str(),
            "minecraft:potion" | "minecraft:splash_potion" | "minecraft:lingering_potion"
        )
}
fn lore(h: &dyn Host, s: &mut ItemStack) {
    let mut lines = vec![colored("§7Modified")];
    let material = text(s, "Core");
    if !material.is_empty() && material != "Flint" {
        lines.push(colored(format!("§7Core: {material}")));
    }
    for a in abilities(s) {
        let (color, label) = match a.as_str() {
            "bounce" => ("§a", "Bouncy (3 ricochets)"),
            "fire" => ("§6", "Burning"),
            "frost" => ("§b", "Freezing"),
            "slow" => ("§b", "Slowing"),
            "glow" => ("§e", "Glowing"),
            "poison" => ("§2", "Poisonous"),
            "levitation" => ("§d", "Levitating"),
            "blindness" => ("§8", "Blinding"),
            "weakness" => ("§5", "Weakening"),
            "weight" => ("§7", "Weighted"),
            "swift" => ("§a", "Swift"),
            _ => ("§f", "Knockback"),
        };
        lines.push(colored(format!("{color}{label}")));
    }
    if component(s, "minecraft:potion_contents").is_some() {
        lines.push(colored("§dPotion effects"));
    }
    if field(&data(s), "Dyes").is_some() {
        lines.push(colored("§bColored trail"));
    }
    let values = lines.iter().filter_map(|l| h.text_to_nbt(l).ok()).collect();
    patch(s, "minecraft:lore", Tag::List(values));
}
pub fn modify(
    h: &dyn Host,
    input: &ItemStack,
    mods: &[Option<ItemStack>],
    arrow: bool,
) -> Option<ItemStack> {
    if !(if arrow {
        accepts(h, input, false)
    } else {
        core(h, input)
    }) {
        return None;
    }
    let mut out = make(
        h,
        if arrow {
            "useful_stuff:custom_arrow"
        } else {
            "useful_stuff:custom_shot"
        },
        1,
    )?;
    patch(
        &mut out,
        "minecraft:custom_data",
        Tag::Compound(data(input)),
    );
    out.copy_component_from(input, "minecraft:custom_name");
    out.copy_component_from(input, "minecraft:potion_contents");
    if let Some(p) = profiles::profile(&id(h, input)).filter(|p| p.kind == "core") {
        set_data(&mut out, "Core", Tag::String(p.name.into()));
        for (key, value) in [
            ("Damage", p.damage),
            ("Speed", p.speed),
            ("Gravity", p.gravity),
        ] {
            set_data(&mut out, key, Tag::Double(value.to_bits()));
        }
    }
    let mut traits = abilities(input);
    let mut dyes = match field(&data(input), "Dyes") {
        Some(Tag::List(v)) => v.clone(),
        _ => vec![],
    };
    let mut changed = id(h, input) != "minecraft:flint" && !accepts(h, input, false);
    for m in mods.iter().flatten() {
        let name = id(h, m);
        if name.ends_with("_dye") {
            let dye = Tag::String(name.as_str().into());
            if !dyes.contains(&dye) {
                dyes.push(dye);
                changed = true
            }
            continue;
        }
        if matches!(
            name.as_str(),
            "minecraft:potion" | "minecraft:splash_potion" | "minecraft:lingering_potion"
        ) {
            let contents = component(m, "minecraft:potion_contents")
                .cloned()
                .or_else(|| h.item_default_component(m.item(), "minecraft:potion_contents"))?;
            if !contents
                .get("custom_effects")
                .is_some_and(|t| matches!(t,Tag::List(v) if !v.is_empty()))
            {
                return None;
            }
            patch(&mut out, "minecraft:potion_contents", contents);
            changed = true;
            continue;
        }
        let profile = profiles::profile(&name).filter(|p| p.kind == "modifier")?;
        if traits.len() >= 3 || traits.iter().any(|a| a == profile.effect) {
            return None;
        }
        traits.push(profile.effect.to_owned());
        changed = true;
    }
    if !changed {
        return None;
    }
    set_data(
        &mut out,
        "Abilities",
        Tag::List(
            traits
                .into_iter()
                .map(|a| Tag::String(a.as_str().into()))
                .collect(),
        ),
    );
    if !dyes.is_empty() {
        set_data(&mut out, "Dyes", Tag::List(dyes));
    }
    lore(h, &mut out);
    Some(out)
}
pub fn arrows(
    h: &dyn Host,
    tip: &ItemStack,
    shaft: &ItemStack,
    feather: &ItemStack,
) -> Option<ItemStack> {
    if !core(h, tip) || id(h, feather) != "minecraft:feather" {
        return None;
    }
    let (speed, gravity, fire) = match id(h, shaft).as_str() {
        "minecraft:stick" => (1., 1., false),
        "minecraft:bamboo" => (1.1, 0.85, false),
        "minecraft:breeze_rod" => (1.2, 0.65, false),
        "minecraft:blaze_rod" => (1., 1., true),
        _ => return None,
    };
    let mut seed = tip.clone();
    if let Some(p) = profiles::profile(&id(h, tip)).filter(|p| p.kind == "core") {
        set_data(&mut seed, "Core", Tag::String(p.name.into()));
        for (key, value) in [
            ("Damage", p.damage),
            ("Speed", p.speed),
            ("Gravity", p.gravity),
        ] {
            set_data(&mut seed, key, Tag::Double(value.to_bits()));
        }
    }
    let mut out = make(h, "useful_stuff:custom_arrow", 8)?;
    patch(
        &mut out,
        "minecraft:custom_data",
        Tag::Compound(data(&seed)),
    );
    out.copy_component_from(&seed, "minecraft:potion_contents");
    let old_speed = number(&out, "Speed", 1.);
    let old_gravity = number(&out, "Gravity", 1.);
    set_data(
        &mut out,
        "Speed",
        Tag::Double((old_speed * speed).to_bits()),
    );
    set_data(
        &mut out,
        "Gravity",
        Tag::Double((old_gravity * gravity).to_bits()),
    );
    if fire {
        let mut a = abilities(&out);
        if !a.iter().any(|v| v == "fire") {
            if a.len() >= 3 {
                return None;
            }
            a.push("fire".into());
        }
        set_data(
            &mut out,
            "Abilities",
            Tag::List(
                a.into_iter()
                    .map(|a| Tag::String(a.as_str().into()))
                    .collect(),
            ),
        );
    }
    lore(h, &mut out);
    Some(out)
}

pub fn hit(h: &mut dyn ServerHost, ammo: &ItemStack, target: EntityRef) {
    let target_uuid = target.uuid(h);
    for ability in abilities(ammo) {
        match ability.as_str() {
            "fire" => {
                target.set_remaining_fire_ticks(h, 80);
            }
            "frost" => {
                target.set_ticks_frozen(h, 120);
                if let Some(u) = target_uuid {
                    effect(h, u, "slowness", 120, 0)
                }
            }
            "slow" | "poison" | "glow" | "levitation" | "blindness" | "weakness" => {
                if let Some(u) = target_uuid {
                    let (effect_name, ticks) = match ability.as_str() {
                        "slow" => ("slowness", 120),
                        "poison" => ("poison", 80),
                        "glow" => ("glowing", 200),
                        "levitation" => ("levitation", 40),
                        "blindness" => ("blindness", 60),
                        _ => ("weakness", 120),
                    };
                    effect(h, u, effect_name, ticks, 0)
                }
            }
            _ => {}
        }
    }
    if let (Some(u), Some(Tag::List(effects))) = (
        target_uuid,
        component(ammo, "minecraft:potion_contents").and_then(|t| t.get("custom_effects")),
    ) {
        for value in effects {
            let Some(Tag::String(name)) = value.get("id") else {
                continue;
            };
            let duration = match value.get("duration") {
                Some(Tag::Int(v)) => (*v / 8).clamp(1, 400),
                _ => 100,
            };
            let amplifier = match value.get("amplifier") {
                Some(Tag::Byte(v)) => (*v as i32).clamp(0, 1),
                Some(Tag::Int(v)) => (*v).clamp(0, 1),
                _ => 0,
            };
            let name = name.to_string_lossy();
            if name.starts_with("minecraft:") {
                effect(h, u, &name[10..], duration, amplifier)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_original_modifiers_are_known() {
        assert_eq!(profiles::EXAMPLES.len(), 42);
        for item in profiles::EXAMPLES {
            let p = profiles::profile(item).unwrap();
            assert_eq!(p.kind, "modifier");
            assert!(
                [
                    "blindness",
                    "fire",
                    "frost",
                    "glow",
                    "weight",
                    "bounce",
                    "levitation",
                    "poison",
                    "slow",
                    "swift",
                    "weakness",
                    "knockback"
                ]
                .contains(&p.effect)
            );
        }
    }
    #[test]
    fn cores_preserve_original_balance() {
        let p = profiles::profile("minecraft:flint").unwrap();
        assert_eq!(p.damage, 4.);
        assert!(profiles::profile("minecraft:iron_ingot").is_some());
        assert!(profiles::profile("minecraft:dirt").is_none());
    }
}
