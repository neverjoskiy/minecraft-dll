use std::sync::{
    Mutex, OnceLock,
    atomic::{AtomicBool, Ordering},
};

static FRIENDS: OnceLock<Mutex<Vec<String>>> = OnceLock::new();
static NO_FRIEND_DAMAGE_ACTIVE: AtomicBool = AtomicBool::new(false);

fn friends() -> &'static Mutex<Vec<String>> {
    FRIENDS.get_or_init(|| Mutex::new(Vec::new()))
}

pub fn add(name: &str) -> bool {
    let name = normalize_name(name);
    if name.is_empty() {
        return false;
    }

    let mut friends = friends()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if friends
        .iter()
        .any(|friend| friend.eq_ignore_ascii_case(&name))
    {
        return false;
    }

    friends.push(name);
    friends.sort_by_key(|friend| friend.to_ascii_lowercase());
    true
}

pub fn remove(name: &str) -> bool {
    let name = normalize_name(name);
    if name.is_empty() {
        return false;
    }

    let mut friends = friends()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let Some(index) = friends
        .iter()
        .position(|friend| friend.eq_ignore_ascii_case(&name))
    else {
        return false;
    };

    friends.remove(index);
    true
}

pub fn is_friend(name: &str) -> bool {
    let name = normalize_name(name);
    if name.is_empty() {
        return false;
    }

    friends()
        .lock()
        .map(|friends| {
            friends
                .iter()
                .any(|friend| friend.eq_ignore_ascii_case(&name))
        })
        .unwrap_or(false)
}

pub fn list() -> Vec<String> {
    friends()
        .lock()
        .map(|friends| friends.clone())
        .unwrap_or_default()
}

pub fn replace_all(names: Vec<String>) {
    let mut cleaned = names
        .into_iter()
        .map(|name| normalize_name(&name))
        .filter(|name| !name.is_empty())
        .collect::<Vec<_>>();
    cleaned.sort_by_key(|name| name.to_ascii_lowercase());
    cleaned.dedup_by(|a, b| a.eq_ignore_ascii_case(b));

    let mut friends = friends()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *friends = cleaned;
}

pub fn set_no_friend_damage_active(active: bool) {
    NO_FRIEND_DAMAGE_ACTIVE.store(active, Ordering::Release);
}

pub fn no_friend_damage_active() -> bool {
    NO_FRIEND_DAMAGE_ACTIVE.load(Ordering::Acquire)
}

fn normalize_name(name: &str) -> String {
    name.trim().trim_matches('§').to_string()
}
