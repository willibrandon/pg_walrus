-- Test GUC parameter defaults and behavior
-- These tests verify that GUC parameters are correctly registered

-- Test default values
SHOW walrus.enable;
SHOW walrus.max;
SHOW walrus.threshold;

-- Note: GUC context is SIGHUP, so parameters cannot be changed with SET
-- They can only be changed in postgresql.conf or via ALTER SYSTEM
-- We verify this by checking the error message when trying to SET
SET walrus.enable = false;
