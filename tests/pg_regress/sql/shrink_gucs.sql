-- Test shrink GUC parameter defaults and behavior
-- These tests verify that shrink GUC parameters are correctly registered

-- Test default values
SHOW walrus.shrink_enable;
SHOW walrus.shrink_factor;
SHOW walrus.shrink_intervals;
SHOW walrus.min_size;

-- Test that SET fails for SIGHUP context parameters
SET walrus.shrink_enable = false;

-- Test boundary validation via ALTER SYSTEM
-- Note: These will produce errors for invalid values

-- shrink_factor boundaries (min=0.01, max=0.99)
ALTER SYSTEM SET walrus.shrink_factor = 0.0;
ALTER SYSTEM SET walrus.shrink_factor = 1.0;

-- shrink_intervals boundaries (min=1, max=1000)
ALTER SYSTEM SET walrus.shrink_intervals = 0;

-- min_size boundaries (min=2, max=i32::MAX)
ALTER SYSTEM SET walrus.min_size = 1;

-- Reset any changes that might have been made
ALTER SYSTEM RESET walrus.shrink_factor;
ALTER SYSTEM RESET walrus.shrink_intervals;
ALTER SYSTEM RESET walrus.min_size;
