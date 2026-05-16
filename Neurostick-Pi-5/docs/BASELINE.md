# Baseline

Date: 2026-04-26

Branch:

```text
## miki/neurostick-pi5-edge
 M src/engine.rs
 M src/gui.rs
 M src/main.rs
 M src/model/mod.rs
 M src/model/neurogpt.rs
 M src/openbci.rs
 M src/recorder.rs
 M src/types.rs
 M trainer/train_model.py
?? Neurostick-Pi-5/
?? PROCESS_CHECKLIST.md
?? trainer/data_contract.py
?? trainer/test_data_contract.py
```

Existing tests:

```text
cargo test --quiet
running 6 tests
......
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

python -m unittest trainer/test_data_contract.py
Ran 3 tests in 0.174s
OK
```

Known dirty files before Pi 5 work:

```text
 M src/engine.rs
 M src/gui.rs
 M src/main.rs
 M src/model/mod.rs
 M src/model/neurogpt.rs
 M src/openbci.rs
 M src/recorder.rs
 M src/types.rs
 M trainer/train_model.py
?? Neurostick-Pi-5/
?? PROCESS_CHECKLIST.md
?? trainer/data_contract.py
?? trainer/test_data_contract.py
```

Note: the Pi 5 branch was created from `feature/steam-mapping-helper`. Working-tree
modifications above are user WIP for the steam-mapping-helper feature and are
intentionally left untouched on this branch. They will be committed separately
on the steam-mapping-helper branch.
