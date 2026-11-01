"""Package, install and archive the six-operation table CLI."""

import argparse
import csv
import gzip
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import shutil
import subprocess
import tarfile

PRODUCT = 'rsomics-table'
ROOT = Path(__file__).resolve().parents[1]


def identity(path):
    body = Path(path).read_bytes()
    return dict(bytes=len(body), sha256=hashlib.sha256(body).hexdigest())


def save(path, value):
    Path(path).write_text(json.dumps(value, indent=2, sort_keys=True) + '\n')


def require(ok, message):
    if not ok:
        raise ValueError(message)


def package_contract(package):
    parent = Path(package['manifest_path']).parent
    result = {k:v for k,v in package.items() if k not in ('id', 'manifest_path', 'targets')}
    result['targets'] = [{**t, 'src_path': str(Path(t['src_path']).relative_to(parent))} for t in package['targets']]
    return result


def extract_package(source, destination, expected, head):
    destination = Path(destination)
    destination.mkdir(parents=True)
    with tarfile.open(source, 'r:gz') as archive:
        members = archive.getmembers()
        require(len(members) == len({m.name for m in members}), 'duplicate source package member')
        bodies = {}
        for member in members:
            path = PurePosixPath(member.name)
            require(not path.is_absolute() and '..' not in path.parts and path.parts[0] == destination.name,
                'unsafe source package path')
            require(member.isfile() or member.isdir(), 'unsupported source package member')
            if member.isfile():
                name = '/'.join(path.parts[1:])
                mode = expected[name]['mode'] if name in expected else 0o644
                require(name and member.mode == mode, 'source package file mode differs: ' + name)
                bodies[name] = archive.extractfile(member).read()
        require(set(bodies) == set(expected) | {'Cargo.toml.orig', '.cargo_vcs_info.json'},
            'complete source package membership differs')
        require(bodies['Cargo.toml.orig'] == expected['Cargo.toml']['body'], 'original Cargo manifest differs')
        vcs = json.loads(bodies['.cargo_vcs_info.json'])
        require(vcs['git']['sha1'] == head and not vcs['git'].get('dirty', False) and vcs['path_in_vcs'] == '',
            'source package is not the exact clean CI head')
        for name, row in expected.items():
            if name != 'Cargo.toml':
                require(bodies[name] == row['body'], 'source package body differs: ' + name)
        for name, body in bodies.items():
            path = destination / name
            path.parent.mkdir(parents=True, exist_ok=True)
            with path.open('xb') as stream:
                stream.write(body)
            path.chmod(expected[name]['mode'] if name in expected else 0o644)
    return dict(**identity(source), members=len(bodies), original_members=len(expected),
        original_bodies_modes_match=True, source_commit=head, clean_vcs=True)


def source():
    temporary = Path(os.environ['RUNNER_TEMP'])
    delivery = temporary / 'delivery'
    metadata = json.loads((delivery / 'package-metadata.json').read_bytes())
    package, = [p for p in metadata['packages'] if p['name'] == PRODUCT]
    prefix = PRODUCT + '-' + package['version']
    source = Path(metadata['target_directory']) / 'package' / (prefix + '.crate')
    shutil.copy2(source, delivery / source.name)
    head = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip()
    require(head == os.environ['GITHUB_SHA'], 'source head differs from actual CI head')
    require(subprocess.check_output(['git', 'status', '--porcelain'], cwd=ROOT) == b'', 'source worktree is dirty')
    expected = {}
    for row in subprocess.check_output(['git', 'ls-tree', '-rz', head], cwd=ROOT).rstrip(b'\0').split(b'\0'):
        header, name = row.split(b'\t')
        mode, kind, blob = header.decode().split()
        name = name.decode()
        require(kind == 'blob' and mode in ('100644', '100755'), 'unexpected source Git type/mode: ' + name)
        body = (ROOT / name).read_bytes()
        require(hashlib.sha1(b'blob ' + str(len(body)).encode() + b'\0' + body).hexdigest() == blob,
            'source differs from exact Git body: ' + name)
        expected[name] = dict(mode=int(mode, 8) & 0o777, body=body)
    destination = temporary / 'packaged-source' / prefix
    verification = extract_package(source, destination, expected, head)
    unpacked = json.loads(subprocess.check_output(['cargo', 'metadata', '--manifest-path', str(destination / 'Cargo.toml'),
        '--locked', '--offline', '--no-deps', '--format-version', '1']))
    installed, = [p for p in unpacked['packages'] if p['name'] == PRODUCT]
    require(package_contract(installed) == package_contract(package), 'normalized source package metadata differs')
    verification['normalized_metadata_match'] = True
    save(delivery / 'package-verification.json', verification)
    metadata_body = subprocess.check_output(['cargo', 'metadata', '--locked', '--format-version', '1'], cwd=ROOT)
    tree_body = subprocess.check_output(['cargo', 'tree', '--locked', '--target', os.environ['EXPECTED_HOST'],
        '--edges', 'normal,build', '--prefix', 'none', '--format', '{p}'], cwd=ROOT)
    (delivery / 'native-dependencies.metadata.json').write_bytes(metadata_body)
    (delivery / 'native-dependencies.tree.txt').write_bytes(tree_body)
    by_name = {(p['name'], p['version']):p for p in json.loads(metadata_body)['packages']}
    enabled = set()
    for line in tree_body.decode().splitlines():
        name, version = re.match(r'([^ ]+) v([^ ]+)', line).groups()
        if name != PRODUCT:
            enabled.add(by_name[name, version]['id'])
    inventory = json.loads((ROOT / 'LICENSES/provenance.json').read_bytes())
    require(enabled == set(inventory['target_graphs'][os.environ['EXPECTED_HOST']]), 'enabled target dependency inventory differs')
    require(identity(ROOT / 'Cargo.lock')['sha256'] == inventory['lock_sha256'], 'notice inventory lock differs')
    for row in inventory['copied_texts'].values():
        require(identity(ROOT / row['destination']) == {k:row[k] for k in ('bytes', 'sha256')}, 'bundled notice body differs')
    save(delivery / 'native-dependencies.json', dict(target=os.environ['EXPECTED_HOST'], enabled_packages=sorted(enabled),
        inventory_sha256=identity(ROOT / 'LICENSES/provenance.json')['sha256'],
        metadata_sha256=hashlib.sha256(metadata_body).hexdigest(), tree_sha256=hashlib.sha256(tree_body).hexdigest(),
        enabled_normal_build_graph_verified=True, linked_binary_inventory=False))
    with Path(os.environ['GITHUB_ENV']).open('a') as stream:
        stream.write('PACKAGE_DIRECTORY=' + str(destination) + '\n')
        stream.write('PACKAGE_VERSION=' + package['version'] + '\n')
        stream.write('SOURCE_PACKAGE=' + str(delivery / source.name) + '\n')


def table(path, delimiter='\t'):
    with Path(path).open(newline='') as stream:
        return list(csv.reader(stream, delimiter=delimiter))


def smoke(binary):
    binary = Path(binary).resolve(strict=True)
    directory = Path(os.environ['RUNNER_TEMP']) / 'delivery/lifecycle'
    directory.mkdir(parents=True)
    shutil.copy2(binary, directory / 'installed-binary')
    records = []

    def run(arguments, *, succeeds=True):
        command = [str(binary), *map(str, arguments)]
        index = len(records)
        row = dict(argv=command)
        records.append(row)
        save(directory / 'commands.json', records)
        with (directory / f'{index}.stdout').open('xb') as out, (directory / f'{index}.stderr').open('xb') as err:
            result = subprocess.run(command, cwd=directory, stdout=out, stderr=err,
                env=dict(os.environ, NO_COLOR='1'), timeout=120)
        row['returncode'] = result.returncode
        save(directory / 'commands.json', records)
        require((result.returncode == 0) == succeeds, 'installed command exit differs: ' + str(command))
        return directory / f'{index}.stdout', directory / f'{index}.stderr'

    out, _ = run(['--version'])
    require(out.read_text().strip() == PRODUCT + ' ' + os.environ['PACKAGE_VERSION'], 'installed version differs')
    out, _ = run(['--help'])
    commands = out.read_text().split('Commands:\n', 1)[1].split('\n\n', 1)[0]
    require([line.split()[0] for line in commands.splitlines() if line.split()[0] != 'help'] ==
        ['validate', 'select', 'filter', 'sort', 'join', 'groupby'], 'installed stable operation scope differs')
    input_path = directory / 'input.csv'
    input_path.write_text('sample,group,score,note\ns1,ctrl,9,"low, one"\ns2,case,21,"line\ntwo"\n'
        's3,case,30,"say ""yes"""\ns4,ctrl,18,none\ns5,case,24,é\ns6,ctrl,27,ok\n')
    out, _ = run(['--json', 'validate', input_path])
    report = json.loads(out.read_bytes())
    require(report['tool'] == PRODUCT and report['status'] == 'ok' and
        report['result']['records'] == 6 and report['result']['fields'] == 4, 'installed input validation differs')
    selected, filtered = directory / 'selected.tsv', directory / 'filtered.tsv'
    run(['select', '--fields', 'group,sample,score,note', '--output-tsv', '--output', selected, input_path])
    require(table(selected) == [['group','sample','score','note'], ['ctrl','s1','9','low, one'],
        ['case','s2','21','line\ntwo'], ['case','s3','30','say "yes"'], ['ctrl','s4','18','none'],
        ['case','s5','24','é'], ['ctrl','s6','27','ok']], 'installed selection or quoted fields differ')
    run(['filter', '--tsv', '--where', '$score >= 20', '--output', filtered, selected])
    require(table(filtered) == [['group','sample','score','note'], ['case','s2','21','line\ntwo'],
        ['case','s3','30','say "yes"'], ['case','s5','24','é'], ['ctrl','s6','27','ok']], 'installed filter differs')
    sorted_rows = [['group','sample','score','note'], ['case','s3','30','say "yes"'],
        ['ctrl','s6','27','ok'], ['case','s5','24','é'], ['case','s2','21','line\ntwo']]
    for workers in (1, 4):
        run(['sort', '--tsv', '--key', 'score:nr', '--threads', workers,
            '--output', directory / f'sorted-{workers}.tsv', filtered])
        require(table(directory / f'sorted-{workers}.tsv') == sorted_rows, 'installed sort or worker identity differs')
    metadata = directory / 'metadata.tsv'
    metadata.write_text('sample\tbatch\ns3\tA\ns3\tB\ns6\tA\ns2\tC\ns7\tD\n')
    joined = directory / 'joined.tsv'
    run(['join', '--tsv', '--on', 'sample', '--output', joined, directory / 'sorted-4.tsv', metadata])
    require(table(joined) == [['group','sample','score','note','batch'], ['case','s3','30','say "yes"','A'],
        ['case','s3','30','say "yes"','B'], ['ctrl','s6','27','ok','A'], ['case','s2','21','line\ntwo','C']],
        'installed join identities or Cartesian multiplicity differ')
    grouped = directory / 'grouped.tsv.gz'
    run(['groupby', '--tsv', '--group', 'batch', '--aggregate', 'score:sum=total',
        '--aggregate', 'score:count=n', '--output', grouped, joined])
    with gzip.open(grouped, 'rt', newline='') as stream:
        rows = list(csv.reader(stream, delimiter='\t'))
    require(rows == [['batch','total','n'], ['A','57','2'], ['B','30','1'], ['C','21','1']],
        'installed aggregate or gzip result differs')
    out, _ = run(['--json', 'validate', '--tsv', grouped])
    report = json.loads(out.read_bytes())
    require(report['result']['records'] == 3 and report['result']['fields'] == 3 and
        report['result']['compression'] == 'gzip', 'installed gzip validation differs')
    invalid, protected = directory / 'invalid.csv', directory / 'protected.csv'
    invalid.write_text('sample,score\ns1,2\ns2\n')
    protected.write_bytes(b'previous result\n')
    _, err = run(['select', '--fields', 'sample', '--output', protected, invalid], succeeds=False)
    require(protected.read_bytes() == b'previous result\n' and err.stat().st_size > 0,
        'installed malformed-input failure replaced prior output or lost its diagnostic')
    save(directory / 'VERIFIED.json', dict(installed_binary=identity(binary), calls=records,
        input_records=6, filtered_records=4, joined_records=4, grouped_records=3,
        stable_operations=['validate','select','filter','sort','join','groupby'],
        sort_worker_outputs_equal=True, quoted_fields_preserved=True, duplicate_join_keys_preserved=True,
        gzip_output_verified=True, failure_preserves_prior_output=True,
        files={p.name:identity(p) for p in directory.iterdir() if p.is_file()},
        scope='installed six-operation lifecycle only; not new upstream compatibility or performance'))


def bundle(binary):
    temporary = Path(os.environ['RUNNER_TEMP'])
    delivery = temporary / 'delivery'
    binary = Path(binary).resolve(strict=True)
    stem = PRODUCT + '-' + os.environ['PACKAGE_VERSION'] + '-' + os.environ['NATIVE_TARGET']
    folder = delivery / stem
    folder.mkdir()
    shutil.copy2(binary, folder / PRODUCT)
    for name in ('README.md', 'LICENSE-MIT', 'LICENSE-APACHE-2.0', 'THIRD_PARTY_LICENSES.md', 'THIRD_PARTY_NOTICES.md'):
        shutil.copy2(ROOT / name, folder / name)
    shutil.copytree(ROOT / 'LICENSES', folder / 'LICENSES')
    provenance = dict(product=PRODUCT, version=os.environ['PACKAGE_VERSION'], target=os.environ['NATIVE_TARGET'],
        head_sha=os.environ['GITHUB_SHA'], run_id=int(os.environ['GITHUB_RUN_ID']), run_attempt=int(os.environ['GITHUB_RUN_ATTEMPT']),
        native_host=os.environ['EXPECTED_HOST'], rustc=subprocess.check_output(['rustc', '-vV'], text=True),
        binary=identity(binary), lock_sha256=identity(ROOT / 'Cargo.lock')['sha256'],
        source_package=json.loads((delivery / 'package-verification.json').read_bytes()),
        native_dependencies=json.loads((delivery / 'native-dependencies.json').read_bytes()),
        accepted_implementation_base='2bd0fd3698c152bb27e7d0d7635d51fb41655112',
        historical_performance_binary_sha256='6e490331da90475cd2eaa84a7e3fc0cd77af381a8cc049bf70261547ad549f34',
        this_binary_remeasured=False, registry_publication=False, signed_release=False)
    save(folder / 'BUILD.json', provenance)
    for path in [folder, *folder.rglob('*')]:
        path.chmod(0o755 if path.is_dir() or path.name == PRODUCT else 0o644)
    archive = delivery / (stem + '.tar.gz')
    with tarfile.open(archive, 'w:gz') as output:
        output.add(folder, arcname=stem)
    source = Path(os.environ['SOURCE_PACKAGE'])
    (delivery / 'SHA256SUMS').write_text(''.join(identity(path)['sha256'] + '  ' + path.name + '\n' for path in (archive, source)))
    print(json.dumps(provenance, indent=2, sort_keys=True))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('action', choices=('source', 'smoke', 'bundle'))
    parser.add_argument('--binary', type=Path)
    args = parser.parse_args()
    if args.action == 'source':
        source()
    elif args.action == 'smoke':
        smoke(args.binary)
    else:
        bundle(args.binary)


if __name__ == '__main__':
    main()
