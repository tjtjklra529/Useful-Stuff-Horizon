use super::*;

fn distance(a: [f64; 3], b: [f64; 3]) -> f64 {
    a.iter()
        .zip(b)
        .map(|(a, b)| (a - b).powi(2))
        .sum::<f64>()
        .sqrt()
}
fn reflect(v: [f64; 3], normal: [i32; 3]) -> [f64; 3] {
    let dot = (0..3).map(|i| v[i] * normal[i] as f64).sum::<f64>();
    std::array::from_fn(|i| (v[i] - 2. * dot * normal[i] as f64) * 0.8)
}
pub(super) fn carrier(
    h: &dyn Host,
    ammo: &ItemStack,
    owner: u128,
    grasp: i32,
    recover: bool,
    bounces: i32,
    waiting: bool,
) -> ItemStack {
    let mut s = ammo.clone();
    if let Some(original) = ammo.to_nbt(h) {
        set_data(&mut s, "ShotAmmo", Tag::Compound(original));
    }
    set_data(
        &mut s,
        "ShotOwner",
        Tag::String(format!("{owner:032x}").as_str().into()),
    );
    for (name, value) in [
        ("ShotGrasp", grasp),
        ("ShotRecover", i32::from(recover)),
        ("ShotBounces", bounces),
        ("ShotWaiting", i32::from(waiting)),
    ] {
        set_data(&mut s, name, Tag::Int(value));
    }
    s
}
impl UsefulStuff {
    pub(super) fn track_arrow(&self, h: &mut dyn ServerHost, id: u32, level: &str, fresh: bool) {
        if self.state.lock().unwrap().arrows.contains_key(&id) {
            return;
        }
        let Some(entity) = h.entity(id) else { return };
        let Some(nbt) = entity.nbt(h) else { return };
        let Some(Tag::Compound(raw)) = field(&nbt, "item") else {
            return;
        };
        let Some(ammo) = ItemStack::from_nbt(h, raw).filter(|s| is(h, s, "custom_arrow")) else {
            return;
        };
        if fresh {
            if let Some(projectile) = h.projectile(entity) {
                let damage = projectile.base_damage(h).unwrap_or(2.);
                projectile.set_base_damage(h, damage * ammunition::damage(&ammo) as f64 / 4.);
            }
            if let Some(v) = entity.velocity(h) {
                let speed = ammunition::speed(h, &ammo);
                entity.set_velocity(h, v.map(|v| v * speed));
            }
        }
        let bounces = if ammunition::has(&ammo, "bounce") {
            3
        } else {
            0
        };
        self.state.lock().unwrap().arrows.insert(
            id,
            ArrowFlight {
                ammo,
                level: level.to_owned(),
                bounces,
                age: 0,
            },
        );
    }
    pub(super) fn restore_shot(&self, h: &mut dyn ServerHost, e: &EntityTickPostEvent<'_>) {
        if self
            .state
            .lock()
            .unwrap()
            .shots
            .iter()
            .any(|s| s.entity == e.entity && s.level == e.dimension)
        {
            return;
        }
        let Some(entity) = h.entity(e.entity) else {
            return;
        };
        let stack = if let Some(item) = h.item_entity(entity) {
            item.stack(h)
        } else {
            entity.nbt(h).and_then(|v| {
                field(&v, "Item")
                    .or_else(|| field(&v, "item"))
                    .and_then(|t| {
                        if let Tag::Compound(c) = t {
                            ItemStack::from_nbt(h, c)
                        } else {
                            None
                        }
                    })
            })
        };
        let Some(stack) = stack else { return };
        let Ok(owner) = u128::from_str_radix(&text(&stack, "ShotOwner"), 16) else {
            return;
        };
        let Some(Tag::Compound(raw)) =
            component(&stack, "minecraft:custom_data").and_then(|v| v.get("ShotAmmo"))
        else {
            return;
        };
        let Some(ammo) = ItemStack::from_nbt(h, raw) else {
            return;
        };
        let waiting = if integer(&stack, "ShotWaiting") > 0 {
            h.item_entity(entity)
                .and_then(|i| i.age(h))
                .unwrap_or(0)
                .max(0) as u32
                + 1
        } else {
            0
        };
        self.state.lock().unwrap().shots.push(Shot {
            entity: e.entity,
            owner,
            ammo,
            level: e.dimension.to_owned(),
            grasp: integer(&stack, "ShotGrasp").clamp(0, 4),
            bounces: integer(&stack, "ShotBounces").clamp(0, 3),
            age: 0,
            waiting,
            at: entity.position(h).unwrap_or([0.; 3]),
            recover: integer(&stack, "ShotRecover") > 0,
        });
    }
    fn return_ammo(&self, h: &mut dyn ServerHost, p: PlayerRef, ammo: ItemStack) {
        for (n, mut q) in p.inventory_slots(h).unwrap_or_default() {
            if !is(h, &q, "quiver") {
                continue;
            }
            let mut items = container(h, &q, 9);
            for pass in 0..2 {
                for i in 0..items.len() {
                    let old = items[i].as_ref().map_or(0, |s| s.count());
                    let cap = h.max_stack_size(ammo.item()).unwrap_or(64);
                    if old + ammo.count() > cap {
                        continue;
                    }
                    if (pass == 0
                        && items[i]
                            .as_ref()
                            .is_some_and(|s| s.is_stackable_with(&ammo)))
                        || (pass == 1 && items[i].is_none())
                    {
                        let previous = items[i].clone();
                        let mut next = ammo.clone();
                        let _ = next.set_count(old + ammo.count());
                        items[i] = Some(next);
                        store(h, &mut q, &items);
                        if p.set_inventory_slot(h, n, Some(q.clone())).is_ok() {
                            return;
                        }
                        items[i] = previous;
                    }
                }
            }
        }
        give(h, p, ammo);
    }
    pub(super) fn tick(&self, h: &mut dyn ServerHost) {
        let pouch = key(h, "things:torch_pouch");
        let (tick, menus) = {
            let mut s = self.state.lock().unwrap();
            s.pouch = pouch;
            s.tick += 1;
            (s.tick, std::mem::take(&mut s.pending_menus))
        };
        for (who, n) in menus {
            if let Some(p) = h.player(who) {
                self.open(h, p, "torch_pouch", n)
            }
        }
        self.rotate_menus(h, tick);
        // Expire double-click candidates. A held button emits use ticks, not new clicks.
        self.state
            .lock()
            .unwrap()
            .clicks
            .retain(|_, c| tick - c.tick <= 10);
    }
    pub(super) fn impact(&self, h: &mut dyn ServerHost, e: &mut ProjectileImpactEvent<'_>) {
        if e.kind == "minecraft:arrow" {
            self.track_arrow(h, e.projectile, e.dimension, false);
        }
        let arrow = self
            .state
            .lock()
            .unwrap()
            .arrows
            .get(&e.projectile)
            .cloned();
        if let Some(mut a) = arrow {
            if e.entity.is_none() && a.bounces > 0 {
                if let Some(entity) = h.entity(e.projectile) {
                    let velocity = reflect(entity.velocity(h).unwrap_or([0.; 3]), e.face);
                    entity.teleport_to(
                        h,
                        std::array::from_fn(|i| e.at[i] + e.face[i] as f64 * 0.08),
                        None,
                    );
                    entity.set_velocity(h, velocity);
                    a.bounces -= 1;
                    self.state.lock().unwrap().arrows.insert(e.projectile, a);
                    e.cancelled = true;
                }
            }
            return;
        }
        let shot = {
            let mut s = self.state.lock().unwrap();
            s.shots
                .iter()
                .position(|s| s.entity == e.projectile && s.level == e.dimension)
                .map(|i| s.shots.remove(i))
        };
        let Some(mut shot) = shot else { return };
        e.cancelled = true;
        let entity = h.entity(e.projectile);
        if let Some(target) = e.entity.and_then(|i| h.entity(i)) {
            let owner = h.player(shot.owner).map(|p| p.entity());
            if target.hurt(
                h,
                "minecraft:thrown",
                ammunition::damage(&shot.ammo),
                entity,
                owner,
            ) == Some(true)
            {
                ammunition::hit(h, &shot.ammo, target);
                if ammunition::has(&shot.ammo, "knockback") {
                    if let (Some(projectile), Some(old)) = (entity, target.velocity(h)) {
                        let v = projectile.velocity(h).unwrap_or([0.; 3]);
                        let length = (v[0] * v[0] + v[2] * v[2]).sqrt().max(0.01);
                        target.set_velocity(
                            h,
                            [
                                old[0] + v[0] / length * 0.6,
                                old[1] + 0.2,
                                old[2] + v[2] / length * 0.6,
                            ],
                        );
                    }
                }
            }
            if let Some(entity) = entity {
                entity.discard(h);
            }
            return;
        }
        if let Some(entity) = entity {
            if shot.bounces > 0 {
                let v = reflect(entity.velocity(h).unwrap_or([0.; 3]), e.face);
                entity.teleport_to(
                    h,
                    std::array::from_fn(|i| e.at[i] + e.face[i] as f64 * 0.08),
                    None,
                );
                entity.set_velocity(h, v);
                shot.bounces -= 1;
                let saved = carrier(
                    h,
                    &shot.ammo,
                    shot.owner,
                    shot.grasp,
                    shot.recover,
                    shot.bounces,
                    false,
                );
                if let Some(raw) = saved.to_nbt(h) {
                    let mut saved = Compound::new();
                    saved.insert("Item".into(), Tag::Compound(raw));
                    entity.merge_nbt(h, saved);
                }
                self.state.lock().unwrap().shots.push(shot);
                return;
            }
            entity.discard(h);
        }
        if shot.recover && shot.grasp > 0 {
            let saved = carrier(h, &shot.ammo, shot.owner, shot.grasp, true, 0, true);
            if let Some(raw) = saved.to_nbt(h) {
                let mut nbt = Compound::new();
                nbt.insert("Item".into(), Tag::Compound(raw));
                nbt.insert("PickupDelay".into(), Tag::Short(32767));
                nbt.insert("NoGravity".into(), Tag::Byte(1));
                if let Some(id) = h.summon(e.dimension, "minecraft:item", e.at, nbt) {
                    shot.entity = id;
                    shot.waiting = 1;
                    shot.at = e.at;
                    self.state.lock().unwrap().shots.push(shot);
                }
            }
        }
    }
    pub(super) fn level_tick(&self, h: &mut dyn ServerHost, level: LevelRef) {
        let dimension = level.dimension().id(h).unwrap_or_default();
        let (arrows, hits) = {
            let mut state = self.state.lock().unwrap();
            let ids = state
                .arrows
                .iter()
                .filter(|(_, a)| a.level == dimension)
                .map(|(k, _)| *k)
                .collect::<Vec<_>>();
            let arrows = ids
                .into_iter()
                .filter_map(|id| state.arrows.remove(&id).map(|a| (id, a)))
                .collect::<Vec<_>>();
            let mut hits = vec![];
            let mut i = 0;
            while i < state.arrow_hits.len() {
                if state.arrow_hits[i].0 == dimension {
                    hits.push(state.arrow_hits.remove(i));
                } else {
                    i += 1
                }
            }
            (arrows, hits)
        };
        for (_, target, ammo) in hits {
            if let Some(entity) = h.entity(target) {
                ammunition::hit(h, &ammo, entity);
            }
        }
        for (id, mut arrow) in arrows {
            let Some(entity) = h.entity(id) else { continue };
            arrow.age += 1;
            if arrow.age > 1200 {
                continue;
            }
            if let Some(v) = entity.velocity(h) {
                if v.iter().any(|v| v.abs() > 0.001) {
                    let g = ammunition::gravity(&arrow.ammo);
                    if (g - 1.).abs() > 0.001 {
                        entity.set_velocity(h, [v[0], v[1] - 0.05 * (g - 1.), v[2]]);
                    }
                    if arrow.age % 2 == 0 {
                        if let (Some(ty), Some(color), Some(at)) = (
                            self.state.lock().unwrap().dust,
                            trail_color(&arrow.ammo, arrow.age),
                            entity.position(h),
                        ) {
                            let mut options = ParticleOptions::simple(ty);
                            options.color = color;
                            options.scale = 0.65;
                            level.send_particles(h, &options, None, at, [0.015; 3], 0., 1);
                        }
                    }
                }
            }
            self.state.lock().unwrap().arrows.insert(id, arrow);
        }
        let shots = {
            let mut state = self.state.lock().unwrap();
            let mut selected = vec![];
            let mut i = 0;
            while i < state.shots.len() {
                if state.shots[i].level == dimension {
                    selected.push(state.shots.remove(i));
                } else {
                    i += 1
                }
            }
            selected
        };
        let mut remaining = vec![];
        for mut shot in shots {
            if shot.waiting > 0 {
                let Some(entity) = h.entity(shot.entity) else {
                    continue;
                };
                shot.waiting += 1;
                let delay = [0, 80, 60, 40, 20][shot.grasp.clamp(0, 4) as usize];
                if shot.waiting >= delay {
                    if let Some(p) = h.player(shot.owner) {
                        let range = [0., 16., 24., 32., 48.][shot.grasp.clamp(0, 4) as usize];
                        if p.dimension(h).is_some_and(|d| d.to_string() == dimension)
                            && p.entity()
                                .position(h)
                                .is_some_and(|pos| distance(pos, shot.at) <= range)
                        {
                            self.return_ammo(h, p, shot.ammo);
                            entity.discard(h);
                            continue;
                        }
                    }
                }
                if shot.waiting < 1200 {
                    remaining.push(shot)
                } else if let Some(item) = h.item_entity(entity) {
                    item.set_stack(h, shot.ammo);
                    item.set_pickup_delay(h, 0);
                    entity.set_no_gravity(h, false);
                }
                continue;
            }
            let Some(entity) = h.entity(shot.entity) else {
                continue;
            };
            shot.age += 1;
            if shot.age > 400 {
                entity.discard(h);
                continue;
            }
            if let Some(v) = entity.velocity(h) {
                let g = ammunition::gravity(&shot.ammo);
                if (g - 1.).abs() > 0.001 {
                    entity.set_velocity(h, [v[0], v[1] - 0.03 * (g - 1.), v[2]]);
                }
            }
            if shot.age % 2 == 0 {
                if let (Some(ty), Some(color), Some(at)) = (
                    self.state.lock().unwrap().dust,
                    trail_color(&shot.ammo, shot.age),
                    entity.position(h),
                ) {
                    let mut options = ParticleOptions::simple(ty);
                    options.color = color;
                    options.scale = 0.65;
                    level.send_particles(h, &options, None, at, [0.015; 3], 0., 1);
                }
            }
            remaining.push(shot);
        }
        self.state.lock().unwrap().shots.extend(remaining);
        let tick = self.state.lock().unwrap().tick;
        let places = {
            let mut s = self.state.lock().unwrap();
            let ids = s
                .placements
                .iter()
                .filter(|(_, v)| tick - v.tick > 10)
                .map(|(k, _)| *k)
                .collect::<Vec<_>>();
            ids.into_iter()
                .filter_map(|k| s.placements.remove(&k))
                .collect::<Vec<_>>()
        };
        for pending in places {
            if let Some(p) = h.player(pending.player) {
                if p.level(h) == Some(level) {
                    self.place_torch(h, p, level, pending);
                }
            }
        }
        if tick % 4 == 0 {
            for p in level.online_players(h).unwrap_or_default() {
                let Some((n, magnet)) = p.inventory_slots(h).and_then(|v| {
                    v.into_iter().find(|(n, s)| {
                        (*n == 40 || Some(*n as usize) == p.selected_slot(h)) && is(h, s, "magnet")
                    })
                }) else {
                    continue;
                };
                let Some(pos) = p.entity().position(h) else {
                    continue;
                };
                let mut pulled = false;
                for entity in level
                    .entities_in(h, pos.map(|v| v - 3.), pos.map(|v| v + 3.))
                    .unwrap_or_default()
                {
                    if h.item_entity(entity).is_none() {
                        continue;
                    }
                    let Some(at) = entity.position(h) else {
                        continue;
                    };
                    let length = distance(pos, at);
                    if length > 3. || length < 0.3 {
                        continue;
                    }
                    entity
                        .set_velocity(h, std::array::from_fn(|i| (pos[i] - at[i]) / length * 0.3));
                    pulled = true;
                }
                if pulled && tick % 20 == 0 {
                    wear(h, p, n, magnet);
                }
            }
        }
    }
    fn place_torch(
        &self,
        h: &mut dyn ServerHost,
        p: PlayerRef,
        level: LevelRef,
        click: PendingPlace,
    ) {
        if p.may_build(h) != Some(true) || p.is_spectator(h) == Some(true) {
            return;
        }
        let Some(mut pouch) = slot(h, p, click.slot).filter(|s| is(h, s, "torch_pouch")) else {
            return;
        };
        let mut items = container(h, &pouch, 4);
        let Some(index) = items
            .iter()
            .position(|s| s.as_ref().is_some_and(|s| id(h, s) == "minecraft:torch"))
        else {
            return;
        };
        let torch = items[index].as_ref().unwrap();
        let face = match click.hit.face {
            [0, 1, 0] => BlockFace::Up,
            [1, 0, 0] => BlockFace::East,
            [-1, 0, 0] => BlockFace::West,
            [0, 0, 1] => BlockFace::South,
            [0, 0, -1] => BlockFace::North,
            _ => return,
        };
        let pos = std::array::from_fn(|i| click.pos[i] + click.hit.face[i]);
        if p.entity()
            .eye_position(h)
            .is_none_or(|eye| distance(eye, pos.map(|v| v as f64 + 0.5)) > 6.)
            || p.may_use_item_at(h, pos, face, torch) != Some(true)
        {
            return;
        }
        if level.block_face_sturdy(h, click.pos, face, SupportType::Center) != Some(true) {
            return;
        }
        if level.block(h, pos).is_none_or(|b| {
            !matches!(
                b.id(h).as_str(),
                "minecraft:air" | "minecraft:cave_air" | "minecraft:void_air"
            )
        }) {
            return;
        }
        if let Some([min_x, min_z, max_x, max_z]) = level.world_border(h) {
            if (pos[0] as f64) < min_x
                || pos[0] as f64 >= max_x
                || (pos[2] as f64) < min_z
                || pos[2] as f64 >= max_z
            {
                return;
            }
        }
        let name = match face {
            BlockFace::Up => "minecraft:torch",
            BlockFace::North => "minecraft:wall_torch[facing=north]",
            BlockFace::South => "minecraft:wall_torch[facing=south]",
            BlockFace::East => "minecraft:wall_torch[facing=east]",
            BlockFace::West => "minecraft:wall_torch[facing=west]",
            _ => return,
        };
        let Some(state) = h.block_state_id(name) else {
            return;
        };
        if level.set_block(
            h,
            pos,
            state,
            BlockUpdateFlags::NEIGHBORS | BlockUpdateFlags::CLIENTS,
        ) == Ok(true)
        {
            if !creative(h, p) {
                items[index] = items[index].take().and_then(|s| s.shrink(1));
                store(h, &mut pouch, &items);
                let _ = p.set_inventory_slot(h, click.slot, Some(pouch));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounce_reverses_only_normal() {
        assert_eq!(
            reflect([1., -2., 3.], [0, 1, 0]),
            [0.8, 1.6, 2.4000000000000004]
        );
    }
    #[test]
    fn recall_distance_is_three_dimensional() {
        assert_eq!(distance([0.; 3], [0., 3., 4.]), 5.);
    }
}

fn trail_color(ammo: &ItemStack, tick: u32) -> Option<[f32; 4]> {
    let d = data(ammo);
    let Tag::List(colors) = field(&d, "Dyes")? else {
        return None;
    };
    if colors.is_empty() {
        return None;
    }
    let Tag::String(name) = &colors[(tick / 2) as usize % colors.len()] else {
        return None;
    };
    let rgb = match name.to_string_lossy().as_str() {
        "minecraft:red_dye" => [0.9, 0.15, 0.12],
        "minecraft:orange_dye" => [1., 0.5, 0.12],
        "minecraft:yellow_dye" => [1., 0.9, 0.15],
        "minecraft:lime_dye" => [0.6, 0.95, 0.15],
        "minecraft:green_dye" => [0.2, 0.5, 0.15],
        "minecraft:cyan_dye" => [0.1, 0.65, 0.7],
        "minecraft:light_blue_dye" => [0.4, 0.8, 1.],
        "minecraft:blue_dye" => [0.2, 0.3, 0.85],
        "minecraft:purple_dye" => [0.6, 0.2, 0.8],
        "minecraft:magenta_dye" => [0.85, 0.25, 0.8],
        "minecraft:pink_dye" => [1., 0.55, 0.7],
        "minecraft:brown_dye" => [0.5, 0.3, 0.15],
        "minecraft:black_dye" => [0.12; 3],
        "minecraft:gray_dye" => [0.35; 3],
        "minecraft:light_gray_dye" => [0.7; 3],
        "minecraft:white_dye" => [1.; 3],
        _ => return None,
    };
    Some([rgb[0], rgb[1], rgb[2], 1.])
}
