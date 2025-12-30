### Pillar 1: The Bicameral Kernel ("The Brain")

**Purpose:** To manage resources, execute logic, and bridge the gap between high-level intent (LLM) and low-level execution (GPU).

- **1.1. System 2: The Cognitive Engine (The "Conscious" Layer)**

  - **Purpose:** Intent analysis, complex decision making, and policy setting.

  - **Host:** Dedicated VRAM Partition (Resident LLM).

  - **1.1.1. The Intent Parser:**

    - _Function:_ Translates natural language/visual context into a "Ray Bundle" (Task).

    - _Input:_ Text tokens, Gaze vectors.

    - _Output:_ A `TaskStruct` defining the goal.

  - **1.1.2. The Policy Arbiter:**

    - _Function:_ Determines resource allocation (e.g., "Give the compiler 90% of the GPU").

    - _Input:_ System Load, User Priority.

    - _Output:_ Tuning parameters for System 1.

- **1.2. System 1: The Reflex Engine (The "Subconscious" Layer)**

  - **Purpose:** Millisecond-level execution, pixel drawing, networking.

  - **Host:** Persistent Compute Shader (Rust/SPIR-V).

  - **1.2.1. The Megakernel Loop:**

    - _Function:_ An infinite `while(true)` loop running on thousands of GPU threads.

    - _Task:_ Pops items from the Global Task Queue and executes them.

  - **1.2.2. The Ray-Logic Unit (RLU):**

    - _Function:_ Replaces `if/else` with ray intersections.

    - _Sub-Component:_ **BVH Builder** (Converts code logic into 3D geometry).

    - _Sub-Component:_ **Traversal Kernel** (Uses Hardware RT Cores to find the answer).

- **1.3. The Hardware Abstraction Layer (The "Spine")**

  - **Purpose:** Unified access to heterogeneous hardware.

  - **1.3.1. The Hive Manager:**

    - _Function:_ Detects available GPUs (APU + dGPUs).

    - _Task:_ Implements "Work Stealing" algorithms to distribute rays across PCIe.

  - **1.3.2. Zero-Copy Allocator:**

    - _Function:_ Manages Unified Memory pointers so CPU and GPU read the same RAM without copying.

---

### Pillar 2: The Neural File System ("The Memory")

**Purpose:** To store data by _meaning_ (Semantic), not by _location_ (Address).

- **2.1. The Vector Store (The "Hippocampus")**

  - **Purpose:** Short-term and Long-term semantic storage.

  - **2.1.1. The Embedder:**

    - _Function:_ Automatically converts every file (text, code, image) into a vector (list of numbers) upon creation.

  - **2.1.2. The Indexer (HNSW):**

    - _Function:_ Organizes vectors so similar concepts are stored physically close to each other in VRAM.

- **2.2. The Epiphany Buffer (The "Dream Journal")**

  - **Purpose:** Temporary storage for ideas generated during the "Dream State."

  - **2.2.1. The Validator:**

    - _Function:_ A sandbox environment that tests if a "new idea" actually works before saving it to permanent memory.

---

### Pillar 3: The Sensory Interface ("The Senses")

**Purpose:** To ingest the real world without peripherals.

- **3.1. The Vision Pathway (The "Eyes")**

  - **Purpose:** To understand user attention and intent.

  - **3.1.1. Gaze Tracker:**

    - _Function:_ Calculates the $(x,y)$ coordinate on screen where the user is looking.

  - **3.1.2. Object Recognizer:**

    - _Function:_ Identifies objects (e.g., "User is holding a coffee cup" -> enable 'Break Mode').

- **3.2. The Auditory Pathway (The "Ears")**

  - **Purpose:** Continuous, ambient command listening.

  - **3.2.1. The Whisper Stream:**

    - _Function:_ Real-time transcription buffer.

  - **3.2.2. The Audio-Visual Fuse:**

    - _Function:_ Combines "Look at _that_" (Vision) with "Delete _it_" (Audio) to understand what "it" means.

---

### Pillar 4: The Autonomic System ("The Metabolism")

**Purpose:** To drive the system to improve itself without user input.

- **4.1. The Entropy Monitor (The "Hunger" Sensor)**

  - **Purpose:** Measures system inefficiency.

  - **4.1.1. The Latency Watchdog:**

    - _Function:_ Logs any process that takes >16ms.

  - **4.1.2. The Stagnation Timer:**

    - _Function:_ Triggers "Dream Mode" if user input = 0 for >5 minutes.

- **4.2. The Ouroboros Engine (The "Evolution" Loop)**

  - **Purpose:** Self-refactoring code.

  - **4.2.1. The Mutator:**

    - _Function:_ Takes a function binary and applies genetic mutations (random code changes).

  - **4.2.2. The Arena (The Simulation):**

    - _Function:_ Runs the mutated code against the original code with random inputs.

  - **4.2.3. The Hot-Swapper:**

    - _Function:_ Live-patches the running kernel with the winner.

---

### Relationship Mapping (How it flows)

1.  **Input:** User looks at a file and says "Optimize this."

2.  **Senses:** Vision gets the file ID; Audio gets the command "Optimize."

3.  **Kernel (System 2):** The LLM receives the Context `(File_A, Action_Optimize)`. It decides this is a "Heavy Task."

4.  **Kernel (System 1):** The LLM dispatches 10,000 "Optimization Rays" to the Task Queue.

5.  **HAL:** The Hive Manager sees the queue is full and wakes up the external dGPU to process them.

6.  **Memory:** The dGPU pulls the file data directly from the Vector Store.

7.  **Metabolism:** While this happens, the Autonomic System records the execution time to see if it can do it faster next time.

### Development Roadmap

**Phase 1: The Skeleton (Months 1-3)**

- Build **1.3.2 (Zero-Copy Allocator)**: Prove we can share RAM.

- Build **1.2.1 (Megakernel)**: Get a `while(true)` loop running on the GPU without crashing.

**Phase 2: The Eyes (Months 4-6)**

- Build **3.1 (Vision Pathway)**: Control a mouse cursor with eyes.

- Build **1.1 (System 2)**: Connect a local LLM to the cursor output.

**Phase 3: The Memory (Months 7-9)**

- Build **2.1 (Vector Store)**: Replace the file system.

**Phase 4: The Life (Months 10+)**

- Build **4.2 (Ouroboros)**: Turn on the self-optimization loop.
