use std::collections::HashSet;
use std::sync::LazyLock;

use oxc::ast::ast::Expression;

static GESTURE_HANDLER_GESTURE_OBJECTS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    [
        "Tap",
        "Pan",
        "Pinch",
        "Rotation",
        "Fling",
        "LongPress",
        "ForceTouch",
        "Native",
        "Manual",
        "Race",
        "Simultaneous",
        "Exclusive",
        "Hover",
    ]
    .into_iter()
    .collect()
});

pub static GESTURE_HANDLER_BUILDER_METHODS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    [
        "onBegin",
        "onStart",
        "onEnd",
        "onFinalize",
        "onUpdate",
        "onChange",
        "onTouchesDown",
        "onTouchesMove",
        "onTouchesUp",
        "onTouchesCancelled",
    ]
    .into_iter()
    .collect()
});

pub static GESTURE_HANDLER_OBJECT_HOOKS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    [
        "useTapGesture",
        "usePanGesture",
        "usePinchGesture",
        "useRotationGesture",
        "useFlingGesture",
        "useLongPressGesture",
        "useNativeGesture",
        "useManualGesture",
        "useHoverGesture",
    ]
    .into_iter()
    .collect()
});

/// Checks if a callee expression is a gesture object event callback method.
/// Matches pattern: `Gesture.Foo()[*].onBar` where `[*]` = any number of chained method calls.
pub fn is_gesture_object_event_callback_method(exp: &Expression) -> bool {
    match exp {
        Expression::StaticMemberExpression(member) => {
            GESTURE_HANDLER_BUILDER_METHODS.contains(member.property.name.as_str())
                && contains_gesture_object(&member.object)
        }
        _ => false,
    }
}

fn contains_gesture_object(exp: &Expression) -> bool {
    if is_gesture_object(exp) {
        return true;
    }
    // method chaining: CallExpression -> MemberExpression -> object
    if let Expression::CallExpression(call) = exp {
        if let Expression::StaticMemberExpression(member) = &call.callee {
            return contains_gesture_object(&member.object);
        }
    }
    false
}

fn is_gesture_object(exp: &Expression) -> bool {
    // Matches: Gesture.Tap() etc.
    if let Expression::CallExpression(call) = exp {
        if let Expression::StaticMemberExpression(member) = &call.callee {
            if let Expression::Identifier(obj) = &member.object {
                return obj.name == "Gesture"
                    && GESTURE_HANDLER_GESTURE_OBJECTS.contains(member.property.name.as_str());
            }
        }
    }
    false
}
