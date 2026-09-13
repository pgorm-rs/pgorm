"""Run a versioned campaign profile end to end against an owned fixture.

    PYTHONPATH=security/generative/src target/generative-build/venv/bin/python \\
      security/generative/tests/live_campaign.py --profile smoke

Requires Docker and the extension built by `pgorm_campaign.build`. The command
exits non-zero whenever the run was not a complete success, including when it
found a real defect: a campaign that reports a discrepancy has done its job
and must still fail.
"""

import asyncio
import sys

from pgorm_campaign.campaign_main import main, parse


# [spec:pgorm:req:generative.profiles/test]
# [spec:pgorm:req:generative.verdict/test]
# [spec:pgorm:req:generative.artifacts/test]
if __name__ == "__main__":
    sys.exit(asyncio.run(main(parse())))
