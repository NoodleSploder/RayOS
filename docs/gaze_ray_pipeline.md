# RayOS Gaze → Ray → Scheduler Pipeline

## Goal

## Pipeline Stages

### 1. Sensor ingestion (gaze estimation)

### 2. Temporal smoothing → attention vector

### 3. Ray emission into UI/scene graph

### 4. Attention scoring (probabilistic selection)

### 5. Scheduler injection

## Acceptance Criteria
Gaze → Ray → Scheduler Pipeline
Purpose

Convert raw gaze data into probabilistic attention models that drive intent resolution and scheduling.

1. Architectural Overview

RayOS treats gaze as a continuous sensor field, not a pointer.

Pipeline:

Gaze Samples
 → Fixations
 → Rays
 → Object Intersections
 → Attention Hypotheses
 → Intent Resolution

2. Stage 1: Gaze Sampling
GazeSample
- timestamp_ns
- x_norm (0.0 – 1.0)
- y_norm (0.0 – 1.0)
- confidence


Sampling frequency is independent of the system tick.

3. Stage 2: Fixation Detection

Fixations represent attention, not motion.

Fixation
- center_x
- center_y
- radius
- dwell_ms
- confidence


Rules:

Minimum dwell threshold (e.g., 400–800ms)

Temporal smoothing (EMA)

Micro-saccades ignored

4. Stage 3: Ray Emission

Each fixation emits a cone, not a line.

Ray
- origin: fixation center
- angle
- spread


The cone represents uncertainty and peripheral attention.

5. Stage 4: Object Intersection

UI and world objects are stored in a BVH / spatial index.

Hit
- object_id
- intersection_score
- distance
- occlusion

6. Stage 5: Attention Scoring
FocusHypothesis
- object_id
- probability


Scoring factors:

Intersection score

Dwell time

Object salience

Recent history

Context alignment

Multiple hypotheses may coexist.

7. Scheduler Integration

The scheduler:

Consumes focus hypotheses

Publishes them into the System 2 queue

Does not execute actions directly

8. Failure Handling

No hits → attention is “ambient”

Low confidence → defer action

Conflicting hypotheses → ask user or wait

9. Non-Goals

Pixel-perfect gaze accuracy

Deterministic selection

Mouse emulation as a primary abstraction
