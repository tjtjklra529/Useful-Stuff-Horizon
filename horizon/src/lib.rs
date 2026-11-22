//! Useful Stuff's Horizon implementation. All world writes go through the public mod API.
use mod_api::{self, prelude::*, *};
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
mod ammunition;
mod game;
mod menus;
mod profiles;

#[derive(Clone, Copy)]
pub struct Profile {
    kind: &'static str,
    name: &'static str,
    damage: f64,
    speed: f64,
    gravity: f64,
    effect: &'static str,
    duration: i32,
    strength: f64,
}
impl Profile {
    const fn new(
        kind: &'static str,
        name: &'static str,
        damage: f64,
        speed: f64,
        gravity: f64,
        effect: &'static str,
        duration: i32,
        strength: f64,
    ) -> Self {
        Self {
            kind,
            name,
            damage,
            speed,
            gravity,
            effect,
            duration,
            strength,
        }
    }
}

fn field<'a>(map: &'a Compound, key: &str) -> Option<&'a Tag> {
    map.iter()
        .find(|(k, _)| k.to_string_lossy() == key)
        .map(|(_, v)| v)
}
fn component<'a>(s: &'a ItemStack, key: &str) -> Option<&'a Tag> {
    field(s.components()?.as_nbt(), key)
}
fn data(s: &ItemStack) -> Compound {
    match component(s, "minecraft:custom_data") {
        Some(Tag::Compound(c)) => c.clone(),
        _ => Compound::new(),
    }
}
fn integer(s: &ItemStack, key: &str) -> i32 {
    match field(&data(s), key) {
        Some(Tag::Int(n)) => *n,
        _ => 0,
    }
}
fn text(s: &ItemStack, key: &str) -> String {
    match field(&data(s), key) {
        Some(Tag::String(n)) => n.to_string_lossy(),
        _ => String::new(),
    }
}
fn set_data(s: &mut ItemStack, key: &str, value: Tag) {
    let mut c = data(s);
    c.insert(key.into(), value);
    patch(s, "minecraft:custom_data", Tag::Compound(c));
}
fn patch(s: &mut ItemStack, key: &str, value: Tag) {
    let mut c = Compound::new();
    c.insert(key.into(), value);
    let _ = s.apply_components(&c);
}
fn id(h: &dyn Host, s: &ItemStack) -> String {
    s.item().id(h)
}
fn is(h: &dyn Host, s: &ItemStack, name: &str) -> bool {
    id(h, s) == format!("things:{name}")
}
fn key(h: &dyn Host, name: &str) -> Option<ItemKey> {
    h.item_key(&Identifier::parse(name).ok()?)
}
fn make(h: &dyn Host, name: &str, count: u32) -> Option<ItemStack> {
    ItemStack::new(key(h, name)?, count)
}
fn slot(h: &dyn Host, p: PlayerRef, n: u32) -> Option<ItemStack> {
    p.inventory_slots(h)?
        .into_iter()
        .find(|(i, _)| *i == n)
        .map(|(_, s)| s)
}
fn held_slot(h: &dyn Host, p: PlayerRef, hand: Hand) -> u32 {
    if hand == Hand::Off {
        40
    } else {
        p.selected_slot(h).unwrap_or(0) as u32
    }
}
fn main(h: &dyn Host, p: PlayerRef) -> Option<ItemStack> {
    slot(h, p, p.selected_slot(h)? as u32)
}
fn container(h: &dyn Host, s: &ItemStack, size: usize) -> Vec<Option<ItemStack>> {
    let mut out = vec![None; size];
    if let Some(Tag::List(values)) = component(s, "minecraft:container") {
        for v in values {
            let Some(Tag::Int(index)) = v.get("slot") else {
                continue;
            };
            if *index < 0 || *index as usize >= size {
                continue;
            }
            if let Some(Tag::Compound(value)) = v.get("item") {
                out[*index as usize] = ItemStack::from_nbt(h, value);
            }
        }
    }
    out
}
fn store(h: &dyn Host, s: &mut ItemStack, items: &[Option<ItemStack>]) {
    let entries = items
        .iter()
        .enumerate()
        .filter_map(|(index, item)| {
            Some(Tag::compound([
                ("slot", Tag::Int(index as i32)),
                ("item", Tag::Compound(item.as_ref()?.to_nbt(h)?)),
            ]))
        })
        .collect();
    patch(s, "minecraft:container", Tag::List(entries));
}
fn put(h: &mut dyn ServerHost, place: &ItemPlace, value: Option<ItemStack>) -> Result<(), ()> {
    h.transaction(|h, tx| {
        if let Some(old) = place.stack(h) {
            if place.extract(h, tx, &ItemResource::of(&old), old.count()) != old.count() {
                return Err(());
            }
        }
        if let Some(new) = value {
            if place.insert(h, tx, &ItemResource::of(&new), new.count()) != new.count() {
                return Err(());
            }
        }
        Ok(())
    })
}
fn give(h: &mut dyn ServerHost, p: PlayerRef, mut item: ItemStack) {
    for pass in 0..2 {
        for n in 0..36 {
            let there = slot(h, p, n);
            let capacity = h.max_stack_size(item.item()).unwrap_or(64);
            if pass == 0 && there.as_ref().is_some_and(|s| s.is_stackable_with(&item))
                || pass == 1 && there.is_none()
            {
                let count = there.as_ref().map_or(0, |s| s.count());
                let take = item.count().min(capacity.saturating_sub(count));
                if take == 0 {
                    continue;
                }
                let mut next = item.clone();
                let _ = next.set_count(count + take);
                if p.set_inventory_slot(h, n, Some(next)).is_ok() {
                    if take == item.count() {
                        return;
                    }
                    let _ = item.set_count(item.count() - take);
                }
            }
        }
    }
    let _ = p.drop_stack(h, item, false);
}
fn creative(h: &dyn Host, p: PlayerRef) -> bool {
    p.is_creative(h) == Some(true)
}
fn wear(h: &mut dyn ServerHost, p: PlayerRef, n: u32, mut item: ItemStack) {
    if creative(h, p) {
        return;
    }
    let next = item.damage() + 1;
    if next >= h.max_damage(item.item()).unwrap_or(u32::MAX) {
        let _ = p.set_inventory_slot(h, n, None);
    } else {
        item.set_damage(next);
        let _ = p.set_inventory_slot(h, n, Some(item));
    }
}
fn uuid(value: u128) -> String {
    let s = format!("{value:032x}");
    format!(
        "{}-{}-{}-{}-{}",
        &s[..8],
        &s[8..12],
        &s[12..16],
        &s[16..20],
        &s[20..]
    )
}
fn effect(h: &mut dyn ServerHost, target: u128, name: &str, ticks: i32, amplifier: i32) {
    if Identifier::parse(&format!("minecraft:{name}")).is_err() {
        return;
    }
    let _ = h.commands().run_command(&format!(
        "execute as {} run effect give @s minecraft:{} {} {} true",
        uuid(target),
        name,
        (ticks + 19) / 20,
        amplifier
    ));
}

#[derive(Clone)]
struct Click {
    slot: u32,
    tick: u64,
}
#[derive(Clone)]
struct PendingPlace {
    player: u128,
    slot: u32,
    pos: [i32; 3],
    hit: BlockHit,
    tick: u64,
}
#[derive(Clone)]
struct Mining {
    pos: [i32; 3],
    face: [i32; 3],
}
#[derive(Clone)]
struct Shot {
    entity: u32,
    owner: u128,
    ammo: ItemStack,
    level: String,
    grasp: i32,
    bounces: i32,
    age: u32,
    waiting: u32,
    at: [f64; 3],
    recover: bool,
}
#[derive(Clone)]
struct ArrowFlight {
    ammo: ItemStack,
    level: String,
    bounces: i32,
    age: u32,
}
#[derive(Default)]
struct State {
    tick: u64,
    clicks: HashMap<u128, Click>,
    placements: HashMap<u128, PendingPlace>,
    mining: HashMap<u128, Mining>,
    breaking: HashSet<u128>,
    shots: Vec<Shot>,
    sessions: HashMap<u128, menus::Session>,
    pending_menus: Vec<(u128, u32)>,
    pouch: Option<ItemKey>,
    pose: Option<CustomArmPose>,
    dust: Option<ParticleKey>,
    arrows: HashMap<u32, ArrowFlight>,
    arrow_hits: Vec<(String, u32, ItemStack)>,
}
#[derive(Default)]
struct UsefulStuff {
    state: Mutex<State>,
}
impl UsefulStuff {
    fn click_pouch(&self, player: u128, n: u32) -> bool {
        let mut s = self.state.lock().unwrap();
        let now = s.tick;
        let last = s.clicks.insert(player, Click { slot: n, tick: now });
        if last.is_some_and(|c| c.slot == n && now - c.tick >= 2 && now - c.tick <= 10) {
            s.clicks.remove(&player);
            s.placements.remove(&player);
            true
        } else {
            false
        }
    }
    fn ammo(
        &self,
        h: &dyn Host,
        p: PlayerRef,
        flint: bool,
    ) -> Option<(u32, Option<usize>, ItemStack)> {
        let accepts = |s: &ItemStack| ammunition::accepts(h, s, flint);
        if let Some(s) = slot(h, p, 40).filter(accepts) {
            return Some((40, None, s));
        }
        let inventory = p.inventory_slots(h)?;
        for (n, q) in &inventory {
            if is(h, q, "quiver") {
                let list = container(h, q, 9);
                let selected = integer(q, "Selected").clamp(0, 8) as usize;
                for i in 0..9 {
                    let k = (i + selected) % 9;
                    if let Some(a) = list[k].clone().filter(accepts) {
                        return Some((*n, Some(k), a));
                    }
                }
            }
        }
        inventory
            .into_iter()
            .find(|(_, a)| accepts(a))
            .map(|(n, a)| (n, None, a))
    }
    fn take_ammo(&self, h: &mut dyn ServerHost, p: PlayerRef, flint: bool) -> Option<ItemStack> {
        let (n, k, a) = match self.ammo(h, p, flint) {
            Some(v) => v,
            None if creative(h, p) => {
                return make(
                    h,
                    if flint {
                        "minecraft:flint"
                    } else {
                        "minecraft:arrow"
                    },
                    1,
                );
            }
            _ => return None,
        };
        let mut one = a.clone();
        let _ = one.set_count(1);
        if creative(h, p) {
            return Some(one);
        }
        let remainder = a.shrink(1);
        if let Some(k) = k {
            let mut q = slot(h, p, n)?;
            let mut list = container(h, &q, 9);
            list[k] = remainder;
            store(h, &mut q, &list);
            p.set_inventory_slot(h, n, Some(q)).ok()?;
        } else {
            p.set_inventory_slot(h, n, remainder).ok()?;
        }
        Some(one)
    }
    fn load(&self, h: &mut dyn ServerHost, p: PlayerRef, hand: Hand) {
        let n = held_slot(h, p, hand);
        let Some(mut sling) = slot(h, p, n).filter(|s| is(h, s, "slingshot")) else {
            return;
        };
        if field(&data(&sling), "Loaded").is_some() {
            return;
        }
        let Some(ammo) = self.take_ammo(h, p, true) else {
            return;
        };
        if let Some(tag) = ammo.to_nbt(h) {
            set_data(&mut sling, "Loaded", Tag::Compound(tag));
            set_data(
                &mut sling,
                "CreativeShot",
                Tag::Int(if creative(h, p) { 1 } else { 0 }),
            );
            let _ = p.set_inventory_slot(h, n, Some(sling));
        }
    }
    fn fire(&self, h: &mut dyn ServerHost, p: PlayerRef, hand: Hand) {
        let n = held_slot(h, p, hand);
        let Some(mut sling) = slot(h, p, n).filter(|s| is(h, s, "slingshot")) else {
            return;
        };
        let mut d = data(&sling);
        let Some(Tag::Compound(raw)) = field(&d, "Loaded") else {
            return;
        };
        let Some(ammo) = ItemStack::from_nbt(h, raw) else {
            return;
        };
        let grasp = ammunition::grasp(&sling);
        let recover = integer(&sling, "CreativeShot") == 0 && !creative(h, p);
        let carrier = game::carrier(
            h,
            &ammo,
            p.uuid(),
            grasp,
            recover,
            if ammunition::has(&ammo, "bounce") {
                3
            } else {
                0
            },
            false,
        );
        let Some(living) = h.living(p.entity()) else {
            return;
        };
        let Some(shot) = h.spawn_projectile_from_rotation(
            living,
            &Identifier::parse("things:flint_shot").unwrap(),
            &carrier,
            0.,
            (2.4 * ammunition::speed(h, &ammo)) as f32,
            1.,
        ) else {
            return;
        };
        d.retain(|k, _| k.to_string_lossy() != "Loaded");
        patch(&mut sling, "minecraft:custom_data", Tag::Compound(d));
        let _ = p.set_inventory_slot(h, n, Some(sling.clone()));
        wear(h, p, n, sling);
        let level = p.dimension(h).map(|i| i.to_string()).unwrap_or_default();
        let bounces = if ammunition::abilities(&ammo).contains(&"bounce".to_owned()) {
            3
        } else {
            0
        };
        self.state.lock().unwrap().shots.push(Shot {
            entity: shot.entity().id(),
            owner: p.uuid(),
            ammo,
            level,
            grasp,
            bounces,
            age: 0,
            waiting: 0,
            at: [0.; 3],
            recover,
        });
        let _ = p.entity().play_sound(h, "things:slingshot.fire", 0.65, 1.);
    }
}

impl UsefulStuff {
    fn handle_use_item(
        &self,
        host: HostSide,
        event: &mut UseItemEvent<'_>,
    ) -> Option<InteractionResult> {
        let HostSide::Server(h) = host else {
            return None;
        };
        let p = h.player(event.player)?;
        let n = held_slot(h, p, event.hand);
        if is(h, event.stack, "slingshot") {
            if field(&data(event.stack), "Loaded").is_some() {
                self.fire(h, p, event.hand);
                return Some(InteractionResult::Consume);
            }
            if self.ammo(h, p, true).is_none() && !creative(h, p) {
                return Some(InteractionResult::Fail);
            }
            if let Some(l) = h.living(p.entity()) {
                l.start_using_item(h, event.hand);
            }
            return Some(InteractionResult::Consume);
        }
        if is(h, event.stack, "torch_pouch") {
            if self.click_pouch(p.uuid(), n) {
                self.open(h, p, "torch_pouch", n)
            } else if let Some(l) = h.living(p.entity()) {
                l.start_using_item(h, event.hand);
            }
            return Some(InteractionResult::Consume);
        }
        if is(h, event.stack, "quiver") {
            if p.is_secondary_use_active(h) == Some(true) {
                let mut q = event.stack.clone();
                let contents = container(h, &q, 9);
                let selected = integer(&q, "Selected").clamp(0, 8) as usize;
                for i in 1..=9 {
                    let next = (selected + i) % 9;
                    if contents[next].is_some() {
                        set_data(&mut q, "Selected", Tag::Int(next as i32));
                        break;
                    }
                }
                let _ = p.set_inventory_slot(h, n, Some(q));
            } else {
                self.open(h, p, "quiver", n)
            }
            return Some(InteractionResult::Consume);
        }
        if is(h, event.stack, "magnet") {
            return Some(InteractionResult::Consume);
        }
        None
    }
    fn handle_use_on(
        &self,
        host: HostSide,
        event: &mut UseOnEvent<'_>,
    ) -> Option<InteractionResult> {
        let HostSide::Server(h) = host else {
            return None;
        };
        let p = h.player(event.player)?;
        let level = p.level(h)?;
        if level
            .block(h, event.pos)
            .is_some_and(|b| b.id(h) == "minecraft:fletching_table")
            && p.is_secondary_use_active(h) != Some(true)
        {
            self.open(h, p, "fletching", u32::MAX);
            return Some(InteractionResult::Consume);
        }
        if is(h, event.stack, "torch_pouch") {
            let n = held_slot(h, p, event.hand);
            if self.click_pouch(p.uuid(), n) {
                self.open(h, p, "torch_pouch", n)
            } else {
                let mut s = self.state.lock().unwrap();
                let tick = s.tick;
                s.placements.insert(
                    p.uuid(),
                    PendingPlace {
                        player: p.uuid(),
                        slot: n,
                        pos: event.pos,
                        hit: event.hit.clone(),
                        tick,
                    },
                );
                drop(s);
                if let Some(l) = h.living(p.entity()) {
                    l.start_using_item(h, event.hand);
                }
            }
            return Some(InteractionResult::Consume);
        }
        None
    }
}

#[mod_api::horizon_mod]
impl Hooks for UsefulStuff {
    fn right_click_item(&self, host: HostSide, e: &mut RightClickItemEvent<'_>) -> Flow {
        let c = e.click;
        let mut event = UseItemEvent {
            stack: c.stack,
            hand: c.hand,
            player: c.player,
            player_entity_id: c.player_entity_id,
        };
        if let Some(result) = self.handle_use_item(host, &mut event) {
            *e.cancellation_result = result;
            e.cancelled = true;
            Flow::Cancel
        } else {
            Flow::Continue
        }
    }
    fn right_click_block(&self, host: HostSide, e: &mut RightClickBlockEvent<'_>) -> Flow {
        let HostSide::Server(h) = host else {
            return Flow::Continue;
        };
        let c = e.click;
        if let Some(p) = h.player(c.player) {
            if p.level(h).is_some_and(|l| {
                l.block(h, c.pos)
                    .is_some_and(|b| b.id(h) == "minecraft:fletching_table")
            }) && p.is_secondary_use_active(h) != Some(true)
            {
                self.open(h, p, "fletching", u32::MAX);
                *e.cancellation_result = InteractionResult::Consume;
                e.cancelled = true;
                return Flow::Cancel;
            }
        }
        if let Some(stack) = c.stack {
            let mut event = UseOnEvent {
                stack,
                pos: c.pos,
                face: c.hit.face,
                hit: c.hit,
                hand: c.hand,
                player: c.player,
                player_entity_id: c.player_entity_id,
            };
            if let Some(result) = self.handle_use_on(HostSide::Server(h), &mut event) {
                *e.cancellation_result = result;
                e.cancelled = true;
                return Flow::Cancel;
            }
        }
        Flow::Continue
    }
    fn register(&self, r: &mut dyn Registrar, _: &RegistrationContext) {
        let pose = r.register_arm_pose("things:slingshot_draw", true, true);
        let dust = r.particle_key("minecraft:dust");
        let mut state = self.state.lock().unwrap();
        state.pose = Some(pose);
        state.dust = Some(dust);
    }
    fn get_arm_pose(&self, h: &dyn ClientHost, e: &mut GetArmPoseEvent<'_>) -> Option<ArmPose> {
        if !is(h, e.stack, "slingshot") {
            return None;
        }
        if e.humanoid.use_hand == Some(e.hand) {
            self.state.lock().unwrap().pose.map(ArmPose::Custom)
        } else {
            Some(ArmPose::Item)
        }
    }
    fn arm_pose_transform(&self, _: &dyn ClientHost, e: &mut ArmPoseTransformEvent<'_>) {
        if self.state.lock().unwrap().pose != Some(e.pose) {
            return;
        }
        let t = (e.humanoid.ticks_using_item as f32 / 20.).clamp(0., 1.);
        let left = e.humanoid.main_arm_left ^ (e.humanoid.use_hand == Some(Hand::Off));
        let hold = [-1.35, if left { 0.32 } else { -0.32 }, 0.];
        let pull = [
            -1.25,
            if left {
                -0.7 - 0.35 * t
            } else {
                0.7 + 0.35 * t
            },
            0.,
        ];
        if left {
            e.angles.left = hold;
            e.angles.right = pull
        } else {
            e.angles.right = hold;
            e.angles.left = pull
        }
    }
    fn render_hand(&self, h: &dyn ClientHost, e: &mut RenderHandEvent<'_, '_>) -> Flow {
        if e.render.stack.is_some_and(|s| is(h, s, "slingshot")) {
            if let Some(l) = h
                .entity(e.render.player_entity_id as u32)
                .and_then(|p| h.living(p))
            {
                if l.used_item_hand(h) == Some(e.render.hand)
                    && l.item_use_remaining(h).is_some_and(|n| n > 0)
                {
                    let t = ((72000 - l.item_use_remaining(h).unwrap_or(72000)) as f32 / 20.)
                        .clamp(0., 1.);
                    let sign = if e.render.hand == Hand::Off { -1. } else { 1. };
                    e.render.translation = [-0.10 * sign, 0.04, -0.08 - 0.10 * t];
                    e.render.rotation = [-6., -12. * sign, -8. * sign];
                    e.render.scale = 0.9;
                }
            }
        }
        Flow::Continue
    }

    fn living_entity_use_item_tick(
        &self,
        host: HostSide,
        event: &mut LivingEntityUseItemTickEvent<'_>,
    ) -> Flow {
        if let HostSide::Server(h) = host {
            if is(h, event.stack, "slingshot") && 72000 - event.duration >= 20 {
                if let Some(p) = h
                    .players()
                    .server_players()
                    .into_iter()
                    .find(|p| Some(p.entity().id() as i32) == event.entity)
                {
                    self.load(h, p, event.hand);
                }
            }
        }
        Flow::Continue
    }
    fn living_entity_use_item_stop(
        &self,
        host: HostSide,
        event: &mut LivingEntityUseItemStopEvent<'_>,
    ) -> Flow {
        if let HostSide::Server(h) = host {
            if is(h, event.stack, "slingshot") {
                if let Some(p) = h
                    .players()
                    .server_players()
                    .into_iter()
                    .find(|p| Some(p.entity().id() as i32) == event.entity)
                {
                    self.fire(h, p, event.hand);
                }
            }
        }
        Flow::Continue
    }
    fn left_click_block(&self, h: &mut dyn ServerHost, e: &mut LeftClickBlockEvent) -> Flow {
        if let Some(p) = h.players().acting_player() {
            if main(h, p).is_some_and(|s| is(h, &s, "obsidian_hammer")) {
                self.state.lock().unwrap().mining.insert(
                    p.uuid(),
                    Mining {
                        pos: e.pos,
                        face: e.face,
                    },
                );
            }
        }
        Flow::Continue
    }
    fn player_destroyed_block(&self, h: &mut dyn ServerHost, e: &mut PlayerDestroyedBlockEvent) {
        let Some(p) = h.players().acting_player() else {
            return;
        };
        if !main(h, p).is_some_and(|s| is(h, &s, "obsidian_hammer")) {
            return;
        }
        let aim = {
            let mut s = self.state.lock().unwrap();
            if s.breaking.contains(&p.uuid()) {
                return;
            }
            let a = s.mining.remove(&p.uuid()).filter(|a| a.pos == e.pos);
            if a.is_some() {
                s.breaking.insert(p.uuid());
            }
            a
        };
        let Some(a) = aim.filter(|a| a.pos == e.pos) else {
            return;
        };
        for x in -1..=1 {
            for y in -1..=1 {
                if x == 0 && y == 0 {
                    continue;
                }
                if !main(h, p).is_some_and(|s| is(h, &s, "obsidian_hammer")) {
                    break;
                }
                let offset = if a.face[0] != 0 {
                    [0, x, y]
                } else if a.face[1] != 0 {
                    [x, 0, y]
                } else {
                    [x, y, 0]
                };
                let pos = std::array::from_fn(|i| a.pos[i] + offset[i]);
                p.destroy_block(h, pos, false);
            }
        }
        self.state.lock().unwrap().breaking.remove(&p.uuid());
    }
    fn menu_clicked(&self, h: &mut dyn ServerHost, e: &mut MenuClickedEvent) {
        self.menu_click(h, e);
    }
    fn menu_closed(&self, h: &mut dyn ServerHost, e: &mut MenuClosedEvent) {
        self.close(h, e.uuid, e.window);
    }
    fn item_stacked_on_other(
        &self,
        c: &dyn LoopContext,
        e: &mut ItemStackedOnOtherEvent<'_, '_>,
    ) -> Flow {
        if c.side() != LoopSide::Server {
            return Flow::Continue;
        }
        let v = &mut e.event;
        let Some(who) = v.player else {
            return Flow::Continue;
        };
        if v.primary || v.carried.get().is_some() {
            return Flow::Continue;
        }
        let Some(item) = v.clicked.item() else {
            return Flow::Continue;
        };
        let pouch = self.state.lock().unwrap().pouch;
        if Some(item.item()) != pouch {
            return Flow::Continue;
        }
        let n = v.container_slot as u32;
        if n > 40 {
            return Flow::Continue;
        }
        if self.click_pouch(who, n) {
            self.state.lock().unwrap().pending_menus.push((who, n));
        }
        v.cancellation_result = true;
        Flow::Cancel
    }
    fn server_tick(&self, h: &mut dyn ServerHost, e: &mut ServerTickEvent<'_>) {
        if matches!(
            e.phase,
            ServerTickPhase::ServerTickEnd {
                world_running: true
            }
        ) {
            self.tick(h);
        }
    }
    fn level_tick_post(&self, host: HostSide, e: &mut LevelTickPostEvent) {
        if let HostSide::Server(h) = host {
            self.level_tick(h, e.level);
        }
    }
    fn entity_tick_post(&self, h: &mut dyn ServerHost, e: &mut EntityTickPostEvent<'_>) {
        if e.kind == "things:flint_shot" || e.kind == "minecraft:item" {
            self.restore_shot(h, e);
        } else if e.kind == "minecraft:arrow" {
            self.track_arrow(h, e.entity, e.dimension, false);
        }
    }
    fn projectile_launch(&self, h: &mut dyn ServerHost, e: &mut ProjectileLaunchEvent<'_>) {
        if e.kind == "minecraft:arrow" {
            if let Some(level) = h
                .entity(e.entity)
                .and_then(|v| v.level(h))
                .and_then(|v| v.dimension().id(h))
            {
                self.track_arrow(h, e.entity, &level, true);
            }
        }
    }
    fn living_damage_post(&self, _: &mut dyn LevelRandomHost, e: &mut LivingDamagePostEvent<'_>) {
        if e.container.inflicted() <= 0. {
            return;
        }
        let mut state = self.state.lock().unwrap();
        if let Some(a) = state.arrows.get(&(e.source.direct as u32)).cloned() {
            state.arrow_hits.push((a.level, e.entity as u32, a.ammo));
        }
    }
    fn projectile_impact(&self, h: &mut dyn ServerHost, e: &mut ProjectileImpactEvent<'_>) -> Flow {
        self.impact(h, e);
        if e.cancelled {
            Flow::Cancel
        } else {
            Flow::Continue
        }
    }
    fn living_entity_use_item_finish(
        &self,
        host: HostSide,
        e: &mut LivingEntityUseItemFinishEvent<'_>,
    ) {
        if let HostSide::Server(h) = host {
            if let Some(p) = h
                .players()
                .server_players()
                .into_iter()
                .find(|p| Some(p.entity().id() as i32) == e.entity)
            {
                let buffs: &[(&str, i32, i32)] = if is(h, e.stack, "emerald_apple") {
                    &[
                        ("regeneration", 200, 1),
                        ("resistance", 600, 0),
                        ("speed", 600, 0),
                    ]
                } else if is(h, e.stack, "enchanted_emerald_apple") {
                    &[
                        ("regeneration", 400, 3),
                        ("resistance", 2400, 2),
                        ("absorption", 2400, 4),
                        ("speed", 2400, 1),
                        ("strength", 1200, 1),
                        ("fire_resistance", 9600, 0),
                    ]
                } else {
                    &[]
                };
                for (name, ticks, amp) in buffs {
                    effect(h, p.uuid(), name, *ticks, *amp)
                }
            }
        }
    }
    fn loot_table_load(&self, _: &dyn DataLoadContext, e: &mut LootTableLoadEvent<'_>) {
        if [
            "minecraft:chests/ancient_city",
            "minecraft:chests/jungle_temple",
            "minecraft:chests/stronghold_corridor",
            "minecraft:chests/stronghold_library",
        ]
        .contains(&e.id)
        {
            if let Some(p) = e.table.get_mut("pools").and_then(|p| p.as_array_mut()) {
                p.push(r#"{"rolls":1,"conditions":[{"condition":"minecraft:random_chance","chance":0.015}],"entries":[{"type":"minecraft:item","name":"things:enchanted_emerald_apple"}]}"#.parse().unwrap());
            }
        }
    }
}
