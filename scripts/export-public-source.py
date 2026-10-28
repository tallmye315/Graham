"""Build a reviewed source ZIP without reading Git state or copying local data."""

from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
import re
import tempfile
import zipfile


ROOT_FILES = frozenset({
    '.gitignore', 'AGENTS.md', 'Cargo.lock', 'Cargo.toml', 'GLOSSARY.md',
    'README.md', 'LICENSE', 'THIRD_PARTY_NOTICES.md', 'index.html',
    'package.json', 'package-lock.json', 'tsconfig.json', 'vite.config.ts',
})
SOURCE_DIRS = ('.github', 'assets', 'crates', 'docs', 'market', 'scripts', 'src', 'src-tauri')
SKIP_DIRS = frozenset({
    '.git', 'artifacts', 'node_modules', 'dist', 'target', 'gen', '__pycache__',
    '.venv', '.vite', '.vitest-cache', '.cache', '.pytest_cache', '.mypy_cache',
    '.ruff_cache', 'test-results', 'coverage', '.idea', '.vscode',
})
SKIP_SUFFIXES = frozenset({
    '.pyc', '.pyo', '.log', '.tmp', '.temp', '.bak', '.orig', '.rej', '.old',
    '.exe', '.dll', '.pdb', '.msi', '.cab', '.whl', '.zip', '.7z', '.gz',
    '.pem', '.key', '.p12', '.pfx', '.db', '.sqlite', '.sqlite3', '.grahambak',
    '.csv', '.xlsx', '.xls', '.ofx', '.qfx', '.qif', '.lnk',
})
SKIP_NAMES = frozenset({
    'Thumbs.db', '.DS_Store', '.coverage', '.npmrc', '.netrc', 'auth.json',
    'credentials.json', 'cookies.txt', 'id_rsa', 'id_ed25519',
})
SKIP_NAMES_LOWER = frozenset(name.casefold() for name in SKIP_NAMES)
ENV_EXAMPLES = frozenset({'.env.example', '.env.sample', '.env.template'})
REVIEWED_BINARIES = {
    'crates/ledger/tests/fixtures/v1-ledger.sqlite3':
        '239d615114e6d419f1c835929325fdbd6ab4d2fe2ba81658f6f4039eb5923068',
    'docs/development/previews/ledger-empty.png':
        '489b7e445e69fafe274c5a788662ecbd2c5e4cf2f9dde1531f5fcee4944e914f',
    'docs/development/previews/ledger-reopened.png':
        '2c4035ca2789b739ec138fd5b83f92e5624b658764de514cd8412ea487af4991',
    'src-tauri/icons/icon.ico':
        '04d2774d4cdaf21d21284950e120a1f62bc96362afa81e261eb413a3961e23f7',
    'src-tauri/icons/128x128.png':
        '69d670587cb1dbdf3bda7583336b2620412b6401eae0848843a726115c110a8f',
}
IMAGE_SUFFIXES = frozenset({'.png', '.jpg', '.jpeg', '.gif', '.webp', '.ico', '.bmp'})
SECRET_PATTERN = re.compile(
    r'\b(?:gh[pousr]_[A-Za-z0-9]{20,}|github_pat_[A-Za-z0-9_]{30,}|'
    r'sk-(?:proj-|svcacct-)?[A-Za-z0-9_-]{24,}|sk_live_[A-Za-z0-9]{16,}|'
    r'xox[baprs]-[A-Za-z0-9-]{20,}|AKIA[0-9A-Z]{16})\b|'
    r'-----BEGIN[ ](?:[A-Z0-9]+[ ])?PRIVATE KEY-----'
)
ABSOLUTE_PATTERN = re.compile(r'(?i)\b[a-z]:[\\/]+[^\s\"\'<>`]+|/(?:Users|home)/[^\s\"\'<>`]+')
MOCK_PATHS = ('c' + ':/backups/', 'c' + ':/local/')
PERSONAL_PATTERN = re.compile(r'用户实际使用(?:的)?券商|真实账本(?:无|有).*?已清仓|用户在验收期间自行(?:新增|修改)')


def is_link(path: Path) -> bool:
    return path.is_symlink() or path.is_junction()


def validate_content(name: str, data: bytes) -> None:
    if name in REVIEWED_BINARIES:
        if hashlib.sha256(data).hexdigest() != REVIEWED_BINARIES[name]:
            raise ValueError(f'Reviewed binary changed; review it before exporting: {name}')
        return
    if b'\0' in data:
        raise ValueError(f'Unreviewed binary file: {name}')
    text = data.decode('utf-8-sig')
    if SECRET_PATTERN.search(text):
        raise ValueError(f'Possible secret; export stopped without printing its value: {name}')
    for match in ABSOLUTE_PATTERN.finditer(text):
        mock = name == 'src/backup.test.tsx' and match.group().replace('\\', '/').lower().startswith(MOCK_PATHS)
        if not mock:
            raise ValueError(f'Machine-specific absolute path; export stopped: {name}')
    if PERSONAL_PATTERN.search(text):
        raise ValueError(f'Personal account description; export stopped: {name}')
    if name.endswith('.json'):
        value = json.loads(text)
        if isinstance(value, dict) and any(isinstance(value.get(k), list) for k in ('accounts', 'trades', 'positions', 'deposits')):
            raise ValueError(f'Possible account-data snapshot; review it before exporting: {name}')


def source_files(root: Path) -> list[tuple[str, bytes]]:
    paths = [root / name for name in sorted(ROOT_FILES) if (root / name).exists()]
    for directory in SOURCE_DIRS:
        top = root / directory
        if not top.exists():
            continue
        if is_link(top):
            raise ValueError(f'Source directory is a link: {directory}')
        for base, dirs, files in os.walk(top, followlinks=False):
            kept = []
            for dirname in sorted(dirs):
                if dirname.casefold() in SKIP_DIRS:
                    continue
                path = Path(base) / dirname
                if is_link(path):
                    raise ValueError(f'Linked source directory; export stopped: {path.relative_to(root)}')
                kept.append(dirname)
            dirs[:] = kept
            paths.extend(Path(base) / name for name in sorted(files))
    result = []
    for path in sorted(paths):
        name = path.relative_to(root).as_posix()
        if path.name.casefold() in SKIP_NAMES_LOWER or path.name.startswith('~$'):
            continue
        if path.name.casefold().startswith('.env') and path.name.casefold() not in ENV_EXAMPLES:
            continue
        if path.suffix.lower() in SKIP_SUFFIXES and name not in REVIEWED_BINARIES:
            continue
        if re.search(r'\.(?:db|sqlite3?)-(?:wal|shm|journal)$', path.name, re.I):
            continue
        if path.suffix.lower() in IMAGE_SUFFIXES and name not in REVIEWED_BINARIES:
            raise ValueError(f'Image needs privacy review before exporting: {name}')
        if is_link(path) or not path.resolve().is_relative_to(root):
            raise ValueError(f'Source file is a link or outside the project: {name}')
        data = path.read_bytes()
        validate_content(name, data)
        result.append((name, data))
    names = {name for name, _ in result}
    for required in ('README.md', 'LICENSE', 'package.json', 'package-lock.json', 'Cargo.toml', 'Cargo.lock', 'src-tauri/tauri.conf.json'):
        if required not in names:
            raise ValueError(f'Missing required source file: {required}')
    return result


def export_source(root: Path) -> dict:
    root = root.resolve(strict=True)
    files = source_files(root)
    version = json.loads(dict(files)['package.json'])['version']
    if not re.fullmatch(r'\d+\.\d+\.\d+(?:[+-][A-Za-z0-9.-]+)?', version):
        raise ValueError('Invalid version for output filename')
    for ancestor in (root / 'artifacts', root / 'artifacts/public-release'):
        if is_link(ancestor):
            raise ValueError('Output directory must not be a link')
    output = root / 'artifacts/public-release'
    output.mkdir(parents=True, exist_ok=True)
    target = output / f'Graham-{version}-source.zip'
    with tempfile.NamedTemporaryFile(prefix='source-', suffix='.tmp', dir=output, delete=False) as handle:
        temporary = Path(handle.name)
    try:
        with zipfile.ZipFile(temporary, 'w', compression=zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
            for name, data in files:
                info = zipfile.ZipInfo('Graham/' + name, date_time=(2026, 1, 1, 0, 0, 0))
                info.compress_type = zipfile.ZIP_DEFLATED
                info.external_attr = 0o644 << 16
                archive.writestr(info, data)
        temporary.replace(target)
    finally:
        temporary.unlink(missing_ok=True)
    checksum = hashlib.sha256(target.read_bytes()).hexdigest()
    manifest = {
        'version': version, 'archive': target.name, 'sha256': checksum, 'file_count': len(files),
        'files': [{'path': name, 'sha256': hashlib.sha256(data).hexdigest()} for name, data in files],
    }
    manifest_path = output / f'Graham-{version}-source-manifest.json'
    checksum_path = target.with_suffix('.zip.sha256')
    for path in (manifest_path, checksum_path):
        if is_link(path):
            raise ValueError('Output metadata must not be a link')
    manifest_path.write_text(json.dumps(manifest, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')
    checksum_path.write_text(f'{checksum}  {target.name}\n', encoding='utf-8')
    return {'archive': target.relative_to(root).as_posix(), 'files': len(files), 'bytes': target.stat().st_size, 'sha256': checksum}


if __name__ == '__main__':
    print(json.dumps(export_source(Path(__file__).resolve().parent.parent), ensure_ascii=False))
