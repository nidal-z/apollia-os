"""Shared fixtures for apollia agent tests."""
import sys
import os

# Make agents/ importable as a package from the repo root
sys.path.insert(0, os.path.join(os.path.dirname(__file__), "../.."))
