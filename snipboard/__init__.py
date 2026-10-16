"""Local, synthetic-only Snipboard prototype."""

# Source is watched by the host. Imports must not trigger plugin reload storms.
import sys
sys.dont_write_bytecode = True
