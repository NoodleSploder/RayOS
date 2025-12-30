
RayOS Intent Primitives Specification
Purpose

RayOS does not expose system calls as imperative functions.
Instead, it exposes intent primitives—semantic descriptions of desired outcomes that the system resolves probabilistically and executes continuously.

Intent primitives form the kernel contract between perception, cognition, and execution.

1. Design Principles

Semantic First
Intents describe what should happen, not how.

Probabilistic Resolution
Every intent carries confidence; ambiguity is expected and managed.

Modality-Agnostic
Voice, gaze, text, application signals, and system policies all map into the same envelope.

Explainable by Default
Every resolved intent must be able to answer “why was this executed?”

2. Canonical Intent Envelope
IntentEnvelope
- intent_id: UUID
- timestamp_ns: u64

- source:
    VOICE | TEXT | GAZE | APP | SYSTEM

- verb: IntentVerb

- objects: [IntentObjectRef]

- constraints: [IntentConstraint]

- context: IntentContext

- confidence: f32        // 0.0 – 1.0

- requires_confirmation: bool

- explanation: IntentExplanation


This structure is immutable once resolved and may be logged, replayed, or audited.

3. Intent Verbs (MVP Set)

The MVP intentionally limits the verb set to enforce clarity.

enum IntentVerb {
  FOCUS,
  SELECT,
  OPEN,
  CLOSE,
  ACTIVATE,   // click / press / confirm
  TYPE,
  SCROLL,
  NAVIGATE
}


All higher-order behaviors must be decomposed into these primitives.

4. Object References

RayOS decouples identity from presentation.

IntentObjectRef
- object_id: UUID or stable hash
- kind:
    WINDOW | WIDGET | APP | FILE | TEXT_SPAN | COORDINATE | UNKNOWN

- locator:
    - bbox: (x, y, width, height) optional
    - path: string (semantic path if available)
    - app_id / pid optional

- salience: f32


Object references may be best-effort and refined over time.

5. Constraints

Constraints limit resolution scope and execution behavior.

Examples:

- limit_to_current_context
- do_not_switch_focus
- require_user_presence
- max_latency_ms = 50


Constraints are policy hooks, not guarantees.

6. Context
IntentContext
- focused_object_id
- active_app_id
- recent_intents (sliding window)
- attention_state
- environment_state


Context is supplied by System 1 and updated continuously.

7. Confidence and Confirmation

confidence >= threshold → execute

confidence < threshold → ask for clarification

Confirmation is itself an intent resolution step

RayOS never errors on ambiguity—it negotiates.

8. Explainability
IntentExplanation
- primary_signal: "gaze_fixation" | "voice_reference" | etc.
- contributing_factors: [string]
- confidence_breakdown


Explainability is mandatory for trust and debugging.

9. Non-Goals (Explicit)

POSIX compatibility

Synchronous blocking calls

User-authored imperative scripts at kernel level
