"""The Release API can return a new draft before it appears in release listings."""
import json
import unittest
from unittest.mock import patch
import publish_github


class GitHubPublicationTests(unittest.TestCase):
    @patch.object(publish_github, "gh")
    def test_new_draft_uses_identity_from_create_response(self, gh):
        draft = {"id": 42, "draft": True, "assets": []}
        gh.side_effect = ["", json.dumps(draft)]
        self.assertEqual(publish_github.release_for_tag("v1.2.3", "1.2.3"), draft)
        self.assertIn("POST", gh.call_args.args)

    @patch.object(publish_github, "gh")
    def test_retry_reads_existing_release_by_id(self, gh):
        gh.side_effect = ["42\n", '{"id":42}']
        self.assertEqual(publish_github.release_for_tag("v1.2.3", "1.2.3"), {"id": 42})
        self.assertEqual(gh.call_args.args, ("api", "repos/{owner}/{repo}/releases/42"))
