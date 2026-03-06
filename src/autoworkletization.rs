use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use crate::gesture_handler_autoworkletization::{
    GESTURE_HANDLER_BUILDER_METHODS, GESTURE_HANDLER_OBJECT_HOOKS,
};

static REANIMATED_OBJECT_HOOKS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    let mut set = HashSet::new();
    set.insert("useAnimatedScrollHandler");
    for name in GESTURE_HANDLER_OBJECT_HOOKS.iter() {
        set.insert(name);
    }
    set
});

static REANIMATED_FUNCTION_HOOKS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    [
        "useFrameCallback",
        "useAnimatedStyle",
        "useAnimatedProps",
        "createAnimatedPropAdapter",
        "useDerivedValue",
        "useAnimatedScrollHandler",
        "useAnimatedReaction",
        "withTiming",
        "withSpring",
        "withDecay",
        "withRepeat",
        "runOnUI",
        "executeOnUIRuntimeSync",
        "scheduleOnUI",
        "runOnUISync",
        "runOnUIAsync",
        "runOnRuntime",
        "runOnRuntimeSync",
        "runOnRuntimeAsync",
        "scheduleOnRuntime",
        "runOnRuntimeSyncWithId",
        "scheduleOnRuntimeWithId",
    ]
    .into_iter()
    .collect()
});

static REANIMATED_FUNCTION_ARGS_TO_WORKLETIZE: LazyLock<HashMap<&'static str, &'static [usize]>> =
    LazyLock::new(|| {
        let mut map = HashMap::new();
        map.insert("useFrameCallback", [0].as_slice());
        map.insert("useAnimatedStyle", &[0]);
        map.insert("useAnimatedProps", &[0]);
        map.insert("createAnimatedPropAdapter", &[0]);
        map.insert("useDerivedValue", &[0]);
        map.insert("useAnimatedScrollHandler", &[0]);
        map.insert("useAnimatedReaction", &[0, 1]);
        map.insert("withTiming", &[2]);
        map.insert("withSpring", &[2]);
        map.insert("withDecay", &[1]);
        map.insert("withRepeat", &[3]);
        map.insert("runOnUI", &[0]);
        map.insert("executeOnUIRuntimeSync", &[0]);
        map.insert("scheduleOnUI", &[0]);
        map.insert("runOnUISync", &[0]);
        map.insert("runOnUIAsync", &[0]);
        map.insert("runOnRuntime", &[1]);
        map.insert("runOnRuntimeSync", &[1]);
        map.insert("runOnRuntimeAsync", &[1]);
        map.insert("scheduleOnRuntime", &[1]);
        map.insert("runOnRuntimeSyncWithId", &[1]);
        map.insert("scheduleOnRuntimeWithId", &[1]);

        for name in GESTURE_HANDLER_OBJECT_HOOKS.iter() {
            map.insert(name, [0].as_slice());
        }
        for name in GESTURE_HANDLER_BUILDER_METHODS.iter() {
            map.insert(name, [0].as_slice());
        }
        map
    });

pub fn is_reanimated_function_hook(name: &str) -> bool {
    REANIMATED_FUNCTION_HOOKS.contains(name)
}

pub fn is_reanimated_object_hook(name: &str) -> bool {
    REANIMATED_OBJECT_HOOKS.contains(name)
}

pub fn get_args_to_workletize(name: &str) -> Option<&'static [usize]> {
    REANIMATED_FUNCTION_ARGS_TO_WORKLETIZE.get(name).copied()
}
