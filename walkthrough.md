# Walkthrough - Rust Client Judging Core

I have built a minimal, headless, cross-platform **Rust Client Judging Core** CLI for competitive programming.

The platform follows the client-side judging architectural principle:
> **The client compiles and executes submissions locally and posts candidate outputs & execution telemetry to the server. The server retains test case confidentiality (expected answers are kept on server) and performs authoritative verdict checking.**

---

## 🛠️ Architecture & Module Breakdown

- **`src/config.rs`**: System configuration, persisted to `~/.config/cp-client/config.json`, with auto-detection of system compilers (`g++`/`clang++`, `javac`/`java`, `python3`/`python`).
- **`src/languages/`**: Modular language abstraction supporting **C++**, **Java**, and **Python**, easily extensible for future language additions.
- **`src/judge/`**:
  - `compiler.rs`: Spawns async compiler process in isolated workspace, capturing `stdout`, `stderr`, and compilation timing.
  - `runner.rs`: Spawns process, pipes `stdin` test input, captures `stdout` / `stderr`, enforces output byte limits, and handles process termination on timeouts using Tokio's async process management.
  - `engine.rs`: Coordinates compilation, executes test inputs across workspaces using `tempfile` RAII cleanup, collects OS & toolchain telemetry.
- **`src/api/`**: Async HTTP client (`reqwest`) for authentication, fetching contest information and problem test inputs (without expected outputs), and sending candidate outputs for server verification.
- **`src/contest/`**: Manages contest listing, problem retrieval, and local test input caching (`~/.cache/cp-client/`).
- **`src/mock_server.rs`**: Built-in Axum HTTP mock server for testing end-to-end workflows (login, contest list, problem fetch, authoritative verdict checking) locally without needing an external server setup.
- **`src/main.rs`**: CLI interface powered by `clap`.

---

## 🚀 CLI Commands Implemented

```bash
# 1. Start the embedded mock server for local testing
cargo run -- mock-server --port 8080

# 2. Login to the contest server
cargo run -- login --server http://127.0.0.1:8080 --username alice --password secret

# 3. List available contests
cargo run -- contests

# 4. View specific contest and problem details
cargo run -- contest contest-101
cargo run -- problem A

# 5. Execute code locally against custom input file / stdin
cargo run -- run solution.py --input test.in

# 6. Fetch test inputs, compile & run locally, submit to server for authoritative verdict
cargo run -- submit solution.cpp --problem A --lang cpp
```

---

## 🧪 Verification & End-to-End Demo

### 1. Automated Integration Tests (`cargo test`)
Comprehensive tests located in `tests/integration_tests.rs`:
- Python local execution against input.
- Python process timeout enforcement.
- C++ multi-test-case compilation and execution.
- C++ compilation error handling.

### 2. End-to-End Workflow Demonstration

Given a sample solution for Problem A (Addition):
```cpp
#include <iostream>
using namespace std;

int main() {
    int a, b;
    if (cin >> a >> b) {
        cout << (a + b) << endl;
    }
    return 0;
}
```

Running `cargo run -- submit solution.cpp --problem A --lang cpp` executes the following sequence:
1. Client contacts server at `http://127.0.0.1:8080/api/problems/A/tests` and downloads test inputs `["3 5\n", "-10 25\n", "100 200\n"]`. Note: Expected answers are NOT sent to client!
2. `JudgeEngine` compiles `solution.cpp` using system `g++ -O2`.
3. Client executes binary against each test input and captures outputs: `["8\n", "15\n", "300\n"]`.
4. Client sends output payload and client telemetry to server.
5. Mock server checks candidate outputs against authoritative expected outputs stored in server database.
6. Server returns `AUTHORITATIVE VERDICT: Accepted` (Passed 3 / 3 test cases).
