flowchart TB
    %% =========================
    %% Human & Environment
    %% =========================
    User((Human))
    Sensors[[Sensors<br/>Gaze • Voice • Text • Camera]]
    Env[[External Environment]]

    User --> Sensors
    Env --> Sensors

    %% =========================
    %% System 1: Continuous Simulation
    %% =========================
    subgraph S1[System 1 — Reflex & Simulation Layer]
        KernelLoop[[Persistent Kernel Loop]]
        Perception[Perception Pipeline<br/>Gaze → Fixation → Rays]
        Attention[Attention Model<br/>Focus Hypotheses]
        WorldModel[World / UI State<br/>Spatial Index (BVH)]
    end

    Sensors --> Perception
    Perception --> Attention
    WorldModel <--> Attention
    KernelLoop --> Perception
    KernelLoop --> WorldModel

    %% =========================
    %% System 2: Cognitive Layer
    %% =========================
    subgraph S2[System 2 — Cognitive & Intent Layer]
        IntentResolver[Intent Resolution]
        IntentEnvelope[Intent Envelopes]
        Policy[Policy & Constraints]
        Explain[Explainability & Audit]
    end

    Attention --> IntentResolver
    IntentResolver --> IntentEnvelope
    Policy --> IntentResolver
    IntentEnvelope --> Explain

    %% =========================
    %% Execution & Control Plane
    %% =========================
    subgraph CP[Execution & Control Plane]
        Scheduler[GPU-First Scheduler<br/>Spatial Allocation]
        Executors[Effect Executors<br/>Pointer • Keyboard • Allocation]
    end

    IntentEnvelope --> Scheduler
    Scheduler --> Executors

    %% =========================
    %% Linux Compatibility Projection
    %% =========================
    subgraph LP[Linux Compatibility Projection]
        LinuxVM[Linux VM]
        FrameStream[Frame Capture]
        InputInject[Input Injection]
        Metadata[UI Metadata Channel<br/>(Optional)]
    end

    Executors --> InputInject
    LinuxVM --> FrameStream
    Metadata --> WorldModel
    FrameStream --> WorldModel
    InputInject --> LinuxVM

    %% =========================
    %% Hardware Substrate
    %% =========================
    subgraph HW[Hardware Substrate]
        GPU[GPU<br/>Persistent Compute]
        CPU[CPU<br/>Assist / IO]
        Memory[Shared Memory]
    end

    Scheduler --> GPU
    KernelLoop --> GPU
    Executors --> CPU
    WorldModel --> Memory
    GPU --> Memory
