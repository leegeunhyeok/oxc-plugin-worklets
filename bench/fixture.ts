import Animated, {
  useAnimatedStyle,
  useAnimatedProps,
  useAnimatedReaction,
  useDerivedValue,
  useFrameCallback,
  withTiming,
  withSpring,
  withDecay,
  withRepeat,
  runOnUI,
  runOnJS,
} from 'react-native-reanimated';
import { Gesture } from 'react-native-gesture-handler';

const PI = 3.14159;
const THRESHOLD = 100;

// Simple worklet function
function simpleWorklet(x: number, y: number): number {
  'worklet';
  return x + y;
}

// Async worklet
async function asyncWorklet(url: string): Promise<void> {
  'worklet';
  const result = await fetch(url);
  console.log(result);
}

// Worklet with closure capture
const scale = 2;
const offset = { x: 10, y: 20 };

function workletWithClosure(value: number): number {
  'worklet';
  return value * scale + offset.x + PI;
}

// Arrow function worklet
const arrowWorklet = (a: number, b: number) => {
  'worklet';
  return a * b + THRESHOLD;
};

// Recursive worklet
function fibonacci(n: number): number {
  'worklet';
  if (n <= 1) return n;
  return fibonacci(n - 1) + fibonacci(n - 2);
}

// Worklet with destructuring
function destructuredWorklet({ x, y }: { x: number; y: number }, [a, b]: number[]): number {
  'worklet';
  return x + y + a + b;
}

// Worklet with template literal
function templateWorklet(name: string, count: number): string {
  'worklet';
  return `Hello ${name}, you have ${count} items`;
}

// Worklet with complex expressions
function complexWorklet(items: number[]): number {
  'worklet';
  let sum = 0;
  for (let i = 0; i < items.length; i++) {
    sum += items[i];
  }
  return sum / items.length;
}

// Nested worklet
function outerWorklet(x: number): () => number {
  'worklet';
  const inner = () => {
    'worklet';
    return x * 2;
  };
  return inner;
}

// Component with hooks
function AnimatedComponent(props: { width: number; color: string }) {
  const animatedValue = Animated.useSharedValue(0);
  const { width, color } = props;

  const animatedStyle = useAnimatedStyle(() => {
    return {
      transform: [{ translateX: withSpring(animatedValue.value * width) }],
      opacity: withTiming(animatedValue.value, { duration: 300 }),
      backgroundColor: color,
    };
  });

  const animatedProps = useAnimatedProps(() => {
    return {
      strokeWidth: animatedValue.value * 2,
      fill: color,
    };
  });

  const derivedValue = useDerivedValue(() => {
    return animatedValue.value * scale + offset.y;
  });

  useAnimatedReaction(
    () => animatedValue.value,
    (current: number, previous: number | null) => {
      if (current !== previous) {
        runOnJS(console.log)('Value changed:', current);
      }
    }
  );

  useFrameCallback((frameInfo: { timeSincePreviousFrame: number }) => {
    const delta = frameInfo.timeSincePreviousFrame;
    animatedValue.value += delta * 0.001;
  });

  return null;
}

// Gesture handler workletization
function GestureComponent() {
  const gesture = Gesture.Pan()
    .onStart((event) => {
      console.log('start', event.translationX);
    })
    .onUpdate((event) => {
      const x = event.translationX;
      const y = event.translationY;
      console.log('update', x, y);
    })
    .onEnd((event) => {
      const velocity = event.velocityX;
      console.log('end', velocity);
    });

  return null;
}

// withTiming / withSpring / withDecay / withRepeat
function animationHelpers(sv: Animated.SharedValue<number>) {
  const a = withTiming(sv.value, { duration: 500 });
  const b = withSpring(sv.value, { damping: 10, stiffness: 100 });
  const c = withDecay({ velocity: 1, deceleration: 0.998 });
  const d = withRepeat(withTiming(1, { duration: 1000 }), -1, true);
  return { a, b, c, d };
}

// runOnUI usage
function uiRunner() {
  runOnUI(() => {
    'worklet';
    const value = 42;
    console.log('Running on UI thread:', value);
  })();
}

// Multiple worklets in sequence
function workletA(): number {
  'worklet';
  return 1;
}

function workletB(): number {
  'worklet';
  return 2;
}

function workletC(): number {
  'worklet';
  return workletA() + workletB();
}

// Worklet with try-catch
function safeWorklet(x: number): number {
  'worklet';
  try {
    return 1 / x;
  } catch (e) {
    return 0;
  }
}

// Worklet with conditional
function conditionalWorklet(mode: string, value: number): number {
  'worklet';
  switch (mode) {
    case 'double':
      return value * 2;
    case 'half':
      return value / 2;
    case 'negate':
      return -value;
    default:
      return value;
  }
}

// Worklet with spread and rest
function spreadWorklet(...args: number[]): number {
  'worklet';
  return args.reduce((acc, val) => acc + val, 0);
}

// Worklet referencing other worklets
function composedWorklet(x: number): number {
  'worklet';
  const a = simpleWorklet(x, x);
  const b = arrowWorklet(x, 2);
  return a + b;
}

export {
  simpleWorklet,
  asyncWorklet,
  workletWithClosure,
  arrowWorklet,
  fibonacci,
  destructuredWorklet,
  templateWorklet,
  complexWorklet,
  outerWorklet,
  AnimatedComponent,
  GestureComponent,
  animationHelpers,
  uiRunner,
  workletA,
  workletB,
  workletC,
  safeWorklet,
  conditionalWorklet,
  spreadWorklet,
  composedWorklet,
};
