# Todo List: wt-ssh-manager

## Phase 1 — Foundation
- [ ] **t1-bootstrap** Bootstrap project structure + install deps
- [ ] **t2-crypto** Windows DPAPI crypto module (encrypt/decrypt passwords)
- [ ] **t3-config** Config manager + ServerConfig data model

## Phase 2 — Terminal Integration
- [ ] **t4-fragment** Windows Terminal Fragment JSON generator

## Phase 3 — SSH Session
- [ ] **t5-launcher** Interactive SSH launcher (runs inside WT tab)

## Phase 4 — Full CLI
- [ ] **t6-cli** All click commands: add, list, remove, edit, sync, test, connect

## Phase 5 — Polish
- [ ] **t7-polish** install.bat, error handling, PATH wrapper

## Dependency Graph
```
t1 → t2 → t3 → t4
               ↓
               t5
               ↓
            t6 (needs t4 + t5)
               ↓
               t7
```
