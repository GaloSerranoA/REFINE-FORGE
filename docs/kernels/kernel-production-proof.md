# Kernel Production Proof Checklist

The Kernel agent may emit `human-reviewed` only when the evidence pack
includes:

| Requirement | Evidence |
|---|---|
| Real kernel source | source path and SHA-256 |
| CPU reference | reference implementation or golden output hash |
| Bit-exact run | run report and fixture hash |
| Hardware matrix | GPU, driver, CUDA toolkit, OS, CPU architecture |
| Compiler/runtime metadata | rustc, nvcc, build flags |
| Tolerance policy | exact hash or numeric tolerance justification |
| Performance baseline | latency/throughput and regression threshold |
| HELYX handoff | config, source, and report hashes |
| Human approval | named reviewer, date, decision |
