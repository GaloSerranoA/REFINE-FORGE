-- Refineforge ⇄ HELYX bridge namespace.
--
-- HELYX (C:\HELYX, the operator's production substrate) has its
-- own verified-core under `verified/lean/HELYX/`. This namespace
-- holds REPRESENTATIVE SLICES of HELYX's load-bearing theorems
-- that refineforge verifies + bundles as case-study artifacts.
--
-- Each slice is a small, self-contained copy of a HELYX theorem,
-- imported here so refineforge's lake project can verify it +
-- bundle it. The full integration of HELYX's own lake project is
-- deferred (Phase 2 work).

import Refineforge.Helyx.Audit
