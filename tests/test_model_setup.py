"""Exercise the real POSIX download function with deterministic curl failures."""
import hashlib
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

SETUP = Path(__file__).resolve().parents[1] / 'services/intent-engine/setup.sh'
PAYLOAD = b'verified model fixture\n'
CHECKSUM = hashlib.sha256(PAYLOAD).hexdigest()


class ModelSetupTests(unittest.TestCase):
    def run_download(self, responses, target=None, partial=None):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            if target is not None:
                (root / 'model').write_bytes(target)
            if partial is not None:
                (root / 'model.tmp').write_bytes(partial)
            (root / 'payload').write_bytes(PAYLOAD)
            # Simulate curl's output and status while recording its exact options.
            (root / 'curl').write_text('''#!/bin/sh
printf '%s\\n' "$*" >> "$TEST_ROOT/calls"
n=$(wc -l < "$TEST_ROOT/calls")
eval "response=\\${RESPONSE_$n}"
for arg do output="$arg"; done
case "$response" in
 good) cp "$TEST_ROOT/payload" "$output" ;;
 bad) printf corrupt > "$output" ;;
 range) printf partial > "$output"; exit 33 ;;
 fail) printf partial > "$output"; exit 28 ;;
 *) exit 99 ;;
esac
''')
            (root / 'curl').chmod(0o755)
            source = SETUP.read_text()
            functions = source[source.index('verify_file()'):source.index('download_and_verify "$EMBED_MODEL_URL"')]
            env = dict(os.environ, PATH=f'{root}:' + os.environ['PATH'], TEST_ROOT=str(root))
            env.update({f'RESPONSE_{i}': response for i, response in enumerate(responses, 1)})
            result = subprocess.run(['sh', '-c', 'set -e\n' + functions + '\ndownload_and_verify url "$TEST_ROOT/model" ' + CHECKSUM], env=env, capture_output=True)
            return (result.returncode,
                    (root / 'model').read_bytes() if (root / 'model').exists() else None,
                    (root / 'model.tmp').exists(),
                    (root / 'calls').read_text().splitlines() if (root / 'calls').exists() else [])

    def test_verified_target_is_reused(self):
        self.assertEqual(self.run_download([], target=PAYLOAD), (0, PAYLOAD, False, []))

    def test_verified_partial_is_promoted(self):
        self.assertEqual(self.run_download([], partial=PAYLOAD), (0, PAYLOAD, False, []))

    def test_corrupt_files_are_replaced(self):
        status, target, partial, calls = self.run_download(['good'], target=b'bad', partial=b'bad')
        self.assertEqual((status, target, partial, len(calls)), (0, PAYLOAD, False, 1))

    def test_range_rejection_restarts_without_resume(self):
        status, target, partial, calls = self.run_download(['range', 'good'])
        self.assertEqual((status, target, partial), (0, PAYLOAD, False))
        self.assertIn('-C -', calls[0])
        self.assertNotIn('-C', calls[1])
        self.assertTrue(all('--retry 5 --retry-delay 2' in call for call in calls))

    def test_checksum_mismatch_retries_cleanly(self):
        status, target, partial, calls = self.run_download(['bad', 'good'])
        self.assertEqual((status, target, partial, len(calls)), (0, PAYLOAD, False, 2))

    def test_repeated_checksum_mismatch_fails_without_promotion(self):
        status, target, partial, calls = self.run_download(['bad', 'bad'])
        self.assertEqual((status, target, partial, len(calls)), (1, None, False, 2))

    def test_other_curl_failure_is_propagated(self):
        status, target, _, calls = self.run_download(['fail'])
        self.assertEqual((status, target, len(calls)), (28, None, 1))
        self.assertIn('--retry 5 --retry-delay 2', calls[0])

    def test_restart_failure_is_propagated(self):
        status, target, _, calls = self.run_download(['range', 'fail'])
        self.assertEqual((status, target, len(calls)), (28, None, 2))


if __name__ == '__main__':
    unittest.main()
