"""Test configuration for research_engine.

Sets up mock for thegent.infra.fast_yaml_parser used by topics module.
"""

from __future__ import annotations

import sys
from unittest.mock import MagicMock

# Mock thegent.infra.fast_yaml_parser before any research_engine imports
mock_fyp = MagicMock()
mock_fyp.yaml_load = MagicMock(return_value={"topics": []})
thegent_infra = type(sys)("thegent.infra")
thegent_infra.fast_yaml_parser = mock_fyp
thegent = type(sys)("thegent")
thegent.infra = thegent_infra
sys.modules["thegent"] = thegent
sys.modules["thegent.infra"] = thegent_infra
sys.modules["thegent.infra.fast_yaml_parser"] = mock_fyp
