from .local import LOCAL_VALUE
import pkg.local
import json
from .missing import MISSING_VALUE
import shared

# import pkg.commented
module_name = "pkg.local"
__import__(module_name)
VALUES = (LOCAL_VALUE, pkg.local.LOCAL_VALUE, json, MISSING_VALUE, shared)
