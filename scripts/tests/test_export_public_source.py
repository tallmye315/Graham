"""Verify that private local material cannot enter the public source archive."""

import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
import zipfile

SCRIPT = Path(__file__).resolve().parents[1] / 'export-public-source.py'
SPEC = importlib.util.spec_from_file_location('public_source_export', SCRIPT)
EXPORT = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(EXPORT)


class PublicSourceExportTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='graham-public-export-test-')
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        for name in ('README.md', 'LICENSE', 'package.json', 'package-lock.json', 'Cargo.toml', 'Cargo.lock', 'src-tauri/tauri.conf.json'):
            self.write(name, '{}' if name.endswith('.json') else 'public source\n')
        self.write('package.json', json.dumps({'version': '0.1.23'}))
        self.write('src/main.tsx', 'export const value = 1;\n')

    def write(self, name, value):
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(value, encoding='utf-8')

    def test_private_data_dependencies_and_cache_are_excluded_and_preserved_locally(self):
        private_files = ('artifacts/validation/private.json', 'node_modules/package/index.js',
                         'src/.env.local', 'src/account.sqlite3', 'src/account.sqlite3-wal',
                         'src/.ENV.production', 'src/credentials.JSON', 'src/other.SQLITE3',
                         'src/backup.grahambak', 'scripts/__pycache__/module.pyc')
        for name in private_files:
            self.write(name, 'private-local-only')
        result = EXPORT.export_source(self.root)
        with zipfile.ZipFile(self.root / result['archive']) as archive:
            names = archive.namelist()
            self.assertIn('Graham/src/main.tsx', names)
            self.assertIn('Graham/README.md', names)
            for name in private_files:
                self.assertNotIn('Graham/' + name, names)
        for name in private_files:
            self.assertEqual((self.root / name).read_text(encoding='utf-8'), 'private-local-only')

    def test_unsafe_source_content_stops_export_before_a_package_is_created(self):
        candidates = (
            ('src/token.ts', 'const token = "' + 'ghp_' + 'A' * 36 + '";'),
            ('src/path.ts', 'const path = "' + 'C:/' + 'Users/' + 'example/private";'),
            ('docs/account.json', json.dumps({'accounts': [{'name': 'private account'}]})),
        )
        for name, content in candidates:
            with self.subTest(file=name):
                self.write(name, content)
                with self.assertRaises(ValueError):
                    EXPORT.export_source(self.root)
                self.assertFalse((self.root / 'artifacts/public-release').exists())
                (self.root / name).unlink()

    def test_unreviewed_fixture_or_screenshot_is_not_silently_published(self):
        for name in ('crates/ledger/tests/fixtures/v1-ledger.sqlite3', 'docs/screenshot.png'):
            with self.subTest(file=name):
                self.write(name, 'unexpected replacement content')
                with self.assertRaises(ValueError):
                    EXPORT.export_source(self.root)
                (self.root / name).unlink()


if __name__ == '__main__':
    unittest.main()
