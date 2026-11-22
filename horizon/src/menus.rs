use super::*;
pub const TORCH: [usize; 4] = [11, 12, 13, 14];
pub const AMMO: [usize; 9] = [9, 10, 11, 12, 13, 14, 15, 16, 17];
const INPUT: [usize; 6] = [10, 18, 20, 16, 24, 32];
#[derive(Clone)]
pub struct Session {
    pub kind: String,
    pub window: i32,
    pub source: u32,
    pub original: Option<ItemStack>,
    pub slots: Vec<Option<ItemStack>>,
}
fn accepts(h: &dyn Host, kind: &str, s: &ItemStack) -> bool {
    match kind {
        "torch_pouch" => id(h, s) == "minecraft:torch",
        "quiver" => ammunition::accepts(h, s, true) || ammunition::accepts(h, s, false),
        _ => true,
    }
}
fn icon(h: &dyn Host, name: &str) -> Option<ItemStack> {
    let mut s = make(h, name, 1)?;
    patch(
        &mut s,
        "minecraft:tooltip_display",
        Tag::compound([
            ("hide_tooltip", Tag::Byte(1)),
            ("hidden_components", Tag::List(vec![])),
        ]),
    );
    Some(s)
}
fn recipe(h: &dyn Host, s: &Session, arrow: bool) -> Option<(Vec<usize>, ItemStack)> {
    if !arrow {
        let input = s.slots[10].as_ref()?;
        Some((
            vec![10, 18, 20],
            ammunition::modify(h, input, &[s.slots[18].clone(), s.slots[20].clone()], false)?,
        ))
    } else {
        let bottom = s.slots[32].as_ref()?;
        let out = if ammunition::accepts(h, bottom, false) {
            ammunition::modify(h, bottom, &[s.slots[16].clone(), s.slots[24].clone()], true)
        } else {
            ammunition::arrows(h, s.slots[16].as_ref()?, s.slots[24].as_ref()?, bottom)
        }?;
        Some((vec![16, 24, 32], out))
    }
}
fn usable(s: &Session) -> Vec<usize> {
    match s.kind.as_str() {
        "torch_pouch" => TORCH.to_vec(),
        "quiver" => AMMO.to_vec(),
        _ => INPUT.to_vec(),
    }
}
fn paint(h: &dyn Host, s: &mut Session, tick: u64) {
    let allowed = usable(s);
    for i in 0..s.slots.len() {
        if !allowed.contains(&i) {
            s.slots[i] = icon(h, "minecraft:gray_stained_glass_pane");
        }
    }
    if s.kind == "quiver" {
        let selected = s
            .original
            .as_ref()
            .map_or(0, |q| integer(q, "Selected"))
            .clamp(0, 8) as usize;
        for i in 0..9 {
            s.slots[i] = icon(
                h,
                if i == selected {
                    "minecraft:arrow"
                } else {
                    "minecraft:light_gray_stained_glass_pane"
                },
            );
        }
    }
    if s.kind == "fletching" {
        s.slots[1] = icon(h, "minecraft:flint");
        s.slots[7] = icon(h, "minecraft:arrow");
        let example = profiles::EXAMPLES[(tick / 30) as usize % profiles::EXAMPLES.len()];
        s.slots[9] = icon(h, example);
        s.slots[17] = icon(h, example);
        s.slots[28] = recipe(h, s, false)
            .map(|(_, i)| i)
            .or_else(|| icon(h, "minecraft:gray_stained_glass_pane"));
        s.slots[34] = recipe(h, s, true)
            .map(|(_, i)| i)
            .or_else(|| icon(h, "minecraft:gray_stained_glass_pane"));
    }
}
fn inv_slot(raw: usize, top: usize) -> Option<u32> {
    let n = raw.checked_sub(top)?;
    if n < 27 {
        Some((n + 9) as u32)
    } else if n < 36 {
        Some((n - 27) as u32)
    } else {
        None
    }
}
impl UsefulStuff {
    pub(super) fn open(&self, h: &mut dyn ServerHost, p: PlayerRef, kind: &str, source: u32) {
        let original = if source == u32::MAX {
            None
        } else {
            let Some(item) = slot(h, p, source).filter(|s| is(h, s, kind)) else {
                return;
            };
            Some(item)
        };
        let mut s = Session {
            kind: kind.to_owned(),
            window: 0,
            source,
            original: original.clone(),
            slots: vec![None; if kind == "fletching" { 45 } else { 27 }],
        };
        if let Some(item) = original {
            let slots = if kind == "quiver" {
                AMMO.to_vec()
            } else {
                TORCH.to_vec()
            };
            let saved = container(h, &item, slots.len());
            if saved.iter().flatten().any(|i| !accepts(h, kind, i)) {
                return;
            }
            for (index, slot) in slots.into_iter().enumerate() {
                s.slots[slot] = saved[index].clone();
            }
        }
        let tick = self.state.lock().unwrap().tick;
        paint(h, &mut s, tick);
        let title = match kind {
            "quiver" => "Quiver",
            "torch_pouch" => "Torch Pouch",
            _ => "Fletching Table",
        };
        if let Ok(Some(window)) = h.open_menu(
            p.uuid(),
            if kind == "fletching" { 5 } else { 3 },
            Text::literal(title),
            s.slots.clone(),
            MenuReplace::CloseFirst,
        ) {
            s.window = window;
            self.state.lock().unwrap().sessions.insert(p.uuid(), s);
        }
    }
    fn save_session(&self, h: &mut dyn ServerHost, p: PlayerRef, s: &mut Session) -> bool {
        let Some(original) = s.original.as_ref() else {
            return true;
        };
        if slot(h, p, s.source).as_ref() != Some(original) {
            return false;
        }
        let mut next = original.clone();
        let slots = if s.kind == "quiver" {
            AMMO.to_vec()
        } else {
            TORCH.to_vec()
        };
        let items = slots
            .iter()
            .map(|i| s.slots[*i].clone())
            .collect::<Vec<_>>();
        store(h, &mut next, &items);
        if p.set_inventory_slot(h, s.source, Some(next.clone()))
            .is_err()
        {
            return false;
        }
        s.original = Some(next);
        true
    }
    pub(super) fn close(&self, h: &mut dyn ServerHost, who: u128, window: i32) {
        let Some(s) = self.state.lock().unwrap().sessions.remove(&who) else {
            return;
        };
        if s.window != window {
            self.state.lock().unwrap().sessions.insert(who, s);
            return;
        }
        if s.kind == "fletching" {
            if let Some(p) = h.player(who) {
                for slot in INPUT {
                    if let Some(item) = s.slots[slot].clone() {
                        give(h, p, item);
                    }
                }
            }
        }
    }
    pub(super) fn menu_click(&self, h: &mut dyn ServerHost, e: &MenuClickedEvent) {
        let Some(mut s) = self.state.lock().unwrap().sessions.remove(&e.uuid) else {
            return;
        };
        if s.window != e.window {
            self.state.lock().unwrap().sessions.insert(e.uuid, s);
            return;
        }
        let Some(p) = h.player(e.uuid) else { return };
        let Some(menu) = h.menu(e.uuid, e.window) else {
            return;
        };
        if s.original.is_some() && slot(h, p, s.source).as_ref() != s.original.as_ref() {
            h.close_menu(e.uuid);
            return;
        }
        if e.mode != 0 && e.mode != 1 {
            self.state.lock().unwrap().sessions.insert(e.uuid, s);
            return;
        }
        let cursor_place = ItemPlace::PlayerCursor { menu };
        let cursor = menu.carried(h).flatten();
        let raw = e.slot;
        if raw < 0 {
            self.state.lock().unwrap().sessions.insert(e.uuid, s);
            return;
        }
        let raw = raw as usize;
        if raw < s.slots.len() {
            if s.kind == "quiver" && raw < 9 {
                if let Some(mut q) = s.original.clone() {
                    set_data(&mut q, "Selected", Tag::Int(raw as i32));
                    if p.set_inventory_slot(h, s.source, Some(q.clone())).is_ok() {
                        s.original = Some(q);
                    }
                }
            } else if s.kind == "fletching" && (raw == 28 || raw == 34) {
                if let Some((ingredients, out)) = recipe(h, &s, raw == 34) {
                    let capacity = h.max_stack_size(out.item()).unwrap_or(64);
                    let room = cursor.as_ref().is_none_or(|c| {
                        c.is_stackable_with(&out) && c.count() + out.count() <= capacity
                    });
                    if room {
                        let mut next = out.clone();
                        if let Some(c) = cursor {
                            let _ = next.set_count(c.count() + out.count());
                        }
                        if put(h, &cursor_place, Some(next)).is_ok() {
                            for n in ingredients {
                                if let Some(used) = s.slots[n].take() {
                                    let bottle = matches!(
                                        id(h, &used).as_str(),
                                        "minecraft:potion"
                                            | "minecraft:splash_potion"
                                            | "minecraft:lingering_potion"
                                            | "minecraft:honey_bottle"
                                    );
                                    s.slots[n] = used.shrink(1);
                                    if bottle {
                                        if let Some(b) = make(h, "minecraft:glass_bottle", 1) {
                                            give(h, p, b)
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            } else if usable(&s).contains(&raw) {
                let there = s.slots[raw].clone();
                let right = e.button == 1;
                if e.mode == 1 {
                    if let Some(item) = there {
                        give(h, p, item);
                        s.slots[raw] = None;
                    }
                } else if let Some(mut carried) = cursor {
                    if accepts(h, &s.kind, &carried) {
                        let capacity = h.max_stack_size(carried.item()).unwrap_or(64);
                        if there.as_ref().is_none_or(|s| s.is_stackable_with(&carried)) {
                            let held = there.as_ref().map_or(0, |s| s.count());
                            let amount = (if right { 1 } else { carried.count() })
                                .min(capacity.saturating_sub(held));
                            if amount > 0 {
                                let mut next = carried.clone();
                                let _ = next.set_count(held + amount);
                                let remainder = carried.shrink(amount);
                                if put(h, &cursor_place, remainder).is_ok() {
                                    s.slots[raw] = Some(next);
                                }
                            }
                        } else if !right {
                            if put(h, &cursor_place, there).is_ok() {
                                s.slots[raw] = Some(carried);
                            }
                        }
                    }
                } else if let Some(mut item) = there {
                    let take = if right {
                        (item.count() + 1) / 2
                    } else {
                        item.count()
                    };
                    let remainder = item.clone().shrink(take);
                    let _ = item.set_count(take);
                    if put(h, &cursor_place, Some(item)).is_ok() {
                        s.slots[raw] = remainder;
                    }
                }
            }
        } else if let Some(n) = inv_slot(raw, s.slots.len()).filter(|n| *n != s.source) {
            let there = slot(h, p, n);
            let place = ItemPlace::PlayerSlot {
                player: p.uuid(),
                slot: n as i32,
            };
            if e.mode == 1 {
                if let Some(mut item) = there {
                    if accepts(h, &s.kind, &item) {
                        let mut targets = usable(&s);
                        if s.kind == "fletching" {
                            targets = if ammunition::accepts(h, &item, false)
                                || id(h, &item) == "minecraft:feather"
                            {
                                vec![32]
                            } else if ammunition::core(h, &item) {
                                vec![10, 16]
                            } else if [
                                "minecraft:stick",
                                "minecraft:bamboo",
                                "minecraft:breeze_rod",
                                "minecraft:blaze_rod",
                            ]
                            .contains(&id(h, &item).as_str())
                            {
                                vec![24]
                            } else {
                                vec![18, 20, 16, 24]
                            };
                        }
                        let mut changed = s.slots.clone();
                        let mut left = item.count();
                        for target in targets {
                            let current = changed[target].as_ref();
                            if current.is_none_or(|s| s.is_stackable_with(&item)) {
                                let amount = current.map_or(0, |s| s.count());
                                let cap = h.max_stack_size(item.item()).unwrap_or(64);
                                let take = left.min(cap.saturating_sub(amount));
                                if take > 0 {
                                    let mut next = item.clone();
                                    let _ = next.set_count(amount + take);
                                    changed[target] = Some(next);
                                    left -= take;
                                }
                                if left == 0 {
                                    break;
                                }
                            }
                        }
                        let remainder = if left == 0 {
                            None
                        } else {
                            let _ = item.set_count(left);
                            Some(item)
                        };
                        if p.set_inventory_slot(h, n, remainder).is_ok() {
                            s.slots = changed;
                        }
                    }
                }
            } else {
                let right = e.button == 1;
                let mut next_cursor = cursor.clone();
                let mut next_slot = there.clone();
                let mut valid = false;
                match (cursor, there) {
                    (None, Some(mut old)) => {
                        let take = if right {
                            (old.count() + 1) / 2
                        } else {
                            old.count()
                        };
                        next_slot = old.clone().shrink(take);
                        let _ = old.set_count(take);
                        next_cursor = Some(old);
                        valid = true
                    }
                    (Some(old), None) => {
                        let take = if right { 1 } else { old.count() };
                        let mut placed = old.clone();
                        let _ = placed.set_count(take);
                        next_slot = Some(placed);
                        next_cursor = old.shrink(take);
                        valid = true
                    }
                    (Some(old), Some(mut here)) if old.is_stackable_with(&here) => {
                        let cap = h.max_stack_size(old.item()).unwrap_or(64);
                        let take = (if right { 1 } else { old.count() })
                            .min(cap.saturating_sub(here.count()));
                        let _ = here.set_count(here.count() + take);
                        next_slot = Some(here);
                        next_cursor = old.shrink(take);
                        valid = take > 0
                    }
                    (Some(old), Some(here)) if !right => {
                        next_slot = Some(old);
                        next_cursor = Some(here);
                        valid = true
                    }
                    _ => {}
                }
                if valid {
                    let _: Result<(), ()> = h.transaction(|h, _| {
                        put(h, &place, next_slot)?;
                        put(h, &cursor_place, next_cursor)?;
                        Ok(())
                    });
                }
            }
        }
        if !self.save_session(h, p, &mut s) {
            h.close_menu(e.uuid);
            return;
        }
        let tick = self.state.lock().unwrap().tick;
        paint(h, &mut s, tick);
        let _ = h.menu_set_items(e.uuid, e.window, s.slots.clone());
        self.state.lock().unwrap().sessions.insert(e.uuid, s);
    }
    pub(super) fn rotate_menus(&self, h: &mut dyn ServerHost, tick: u64) {
        if tick % 30 != 0 {
            return;
        }
        let menus = {
            let mut st = self.state.lock().unwrap();
            st.sessions
                .iter_mut()
                .filter(|(_, s)| s.kind == "fletching")
                .map(|(p, s)| {
                    paint(h, s, tick);
                    (*p, s.window, s.slots.clone())
                })
                .collect::<Vec<_>>()
        };
        for (p, window, slots) in menus {
            let _ = h.menu_set_items(p, window, slots);
        }
    }
}
