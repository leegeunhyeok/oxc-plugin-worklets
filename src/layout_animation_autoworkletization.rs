use std::collections::HashSet;
use std::sync::LazyLock;

use oxc::ast::ast::Expression;

static ENTRY_EXIT_ANIMATIONS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    [
        "BounceIn",
        "BounceInDown",
        "BounceInLeft",
        "BounceInRight",
        "BounceInUp",
        "BounceOut",
        "BounceOutDown",
        "BounceOutLeft",
        "BounceOutRight",
        "BounceOutUp",
        "FadeIn",
        "FadeInDown",
        "FadeInLeft",
        "FadeInRight",
        "FadeInUp",
        "FadeOut",
        "FadeOutDown",
        "FadeOutLeft",
        "FadeOutRight",
        "FadeOutUp",
        "FlipInEasyX",
        "FlipInEasyY",
        "FlipInXDown",
        "FlipInXUp",
        "FlipInYLeft",
        "FlipInYRight",
        "FlipOutEasyX",
        "FlipOutEasyY",
        "FlipOutXDown",
        "FlipOutXUp",
        "FlipOutYLeft",
        "FlipOutYRight",
        "LightSpeedInLeft",
        "LightSpeedInRight",
        "LightSpeedOutLeft",
        "LightSpeedOutRight",
        "PinwheelIn",
        "PinwheelOut",
        "RollInLeft",
        "RollInRight",
        "RollOutLeft",
        "RollOutRight",
        "RotateInDownLeft",
        "RotateInDownRight",
        "RotateInUpLeft",
        "RotateInUpRight",
        "RotateOutDownLeft",
        "RotateOutDownRight",
        "RotateOutUpLeft",
        "RotateOutUpRight",
        "SlideInDown",
        "SlideInLeft",
        "SlideInRight",
        "SlideInUp",
        "SlideOutDown",
        "SlideOutLeft",
        "SlideOutRight",
        "SlideOutUp",
        "StretchInX",
        "StretchInY",
        "StretchOutX",
        "StretchOutY",
        "ZoomIn",
        "ZoomInDown",
        "ZoomInEasyDown",
        "ZoomInEasyUp",
        "ZoomInLeft",
        "ZoomInRight",
        "ZoomInRotate",
        "ZoomInUp",
        "ZoomOut",
        "ZoomOutDown",
        "ZoomOutEasyDown",
        "ZoomOutEasyUp",
        "ZoomOutLeft",
        "ZoomOutRight",
        "ZoomOutRotate",
        "ZoomOutUp",
    ]
    .into_iter()
    .collect()
});

static LAYOUT_TRANSITIONS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    [
        "Layout",
        "LinearTransition",
        "SequencedTransition",
        "FadingTransition",
        "JumpingTransition",
        "CurvedTransition",
        "EntryExitTransition",
    ]
    .into_iter()
    .collect()
});

static LAYOUT_ANIMATIONS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    ENTRY_EXIT_ANIMATIONS
        .iter()
        .chain(LAYOUT_TRANSITIONS.iter())
        .copied()
        .collect()
});

static LAYOUT_ANIMATIONS_CHAINABLE_METHODS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    [
        // Base
        "build",
        "duration",
        "delay",
        "getDuration",
        "randomDelay",
        "getDelay",
        "getDelayFunction",
        // Complex
        "easing",
        "rotate",
        "springify",
        "damping",
        "mass",
        "stiffness",
        "overshootClamping",
        "energyThreshold",
        "restDisplacementThreshold",
        "restSpeedThreshold",
        "withInitialValues",
        "getAnimationAndConfig",
        // Default transition
        "easingX",
        "easingY",
        "easingWidth",
        "easingHeight",
        "entering",
        "exiting",
        "reverse",
    ]
    .into_iter()
    .collect()
});

static LAYOUT_ANIMATIONS_CALLBACKS: LazyLock<HashSet<&'static str>> =
    LazyLock::new(|| ["withCallback"].into_iter().collect());

/// Checks if the parent of a function node is a layout animation callback method call.
pub fn is_layout_animation_callback_method(callee: &Expression) -> bool {
    match callee {
        Expression::StaticMemberExpression(member) => {
            LAYOUT_ANIMATIONS_CALLBACKS.contains(member.property.name.as_str())
                && is_layout_animations_chainable_or_new_operator(&member.object)
        }
        _ => false,
    }
}

fn is_layout_animations_chainable_or_new_operator(exp: &Expression) -> bool {
    match exp {
        Expression::Identifier(id) => LAYOUT_ANIMATIONS.contains(id.name.as_str()),
        Expression::NewExpression(new_expr) => {
            if let Expression::Identifier(id) = &new_expr.callee {
                LAYOUT_ANIMATIONS.contains(id.name.as_str())
            } else {
                false
            }
        }
        Expression::CallExpression(call) => {
            if let Expression::StaticMemberExpression(member) = &call.callee {
                LAYOUT_ANIMATIONS_CHAINABLE_METHODS.contains(member.property.name.as_str())
                    && is_layout_animations_chainable_or_new_operator(&member.object)
            } else {
                false
            }
        }
        _ => false,
    }
}
