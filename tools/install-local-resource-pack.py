#!/usr/bin/env python3
"""Install a local licensed pack outside Git, preserving runtime integrity."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess


def outside_git(path):
    probe=path
    while not probe.exists(): probe=probe.parent
    if subprocess.run(['git','-C',str(probe),'rev-parse','--is-inside-work-tree'],capture_output=True).returncode==0:
        raise ValueError('Licensed packs must remain outside Git repositories')


def install(pack, runtime, stores):
    outside_git(runtime)
    for store in stores: outside_git(store)
    manifest_path=runtime/'openpencil-runtime.json'
    manifest=json.loads(manifest_path.read_text())
    # Verify the existing installation before extending it with local data.
    for name,digest in manifest['files'].items():
        if hashlib.sha256((runtime/name).read_bytes()).hexdigest()!=digest:
            raise ValueError('Runtime integrity check failed: '+name)
    catalog=json.loads((pack/'catalog.json').read_text())
    if catalog['version']!=1 or len(catalog['entries'])>24: raise ValueError('Invalid pack')
    ids=set()
    for entry in catalog['entries']:
        slug=entry['id']
        if not slug.startswith('library-') or slug in ids or not all(c.isascii() and (c.isalnum() or c in '-_') for c in slug):
            raise ValueError('Invalid resource id')
        ids.add(slug)
        for key,suffix,limit in [('document','.op',12*1024*1024),('preview','.jpg',2*1024*1024)]:
            name=entry[key]
            if Path(name).name!=name or '..' in name or not name.endswith(suffix):
                raise ValueError('Unsafe resource path')
            source=pack/name
            if source.is_symlink() or source.stat().st_size>limit:
                raise ValueError('Unsafe resource file')
        for store in stores:
            if (store/slug).exists():
                raise ValueError('Refusing to replace an existing saved template')
    destination=runtime/'bin/web-bundle/assets/resource-library'
    destination.mkdir(parents=True,exist_ok=True)
    for entry in catalog['entries']:
        if not entry['id'].startswith('library-'): raise ValueError('Invalid resource id')
        for key,suffix in [('document','.op'),('preview','.jpg')]:
            name=entry[key]
            if Path(name).name!=name or '..' in name or not name.endswith(suffix):
                raise ValueError('Unsafe resource path')
            shutil.copy2(pack/name,destination/name)
        for store in stores:
            target=store/entry['id']
            if target.exists(): raise ValueError('Refusing to replace an existing saved template')
            target.mkdir(parents=True)
            shutil.copy2(pack/entry['document'],target/'document.op')
            shutil.copy2(pack/entry['preview'],target/'preview.jpg')
            (target/'meta.json').write_text(json.dumps({k:entry[k] for k in ['name','frames','frameWidth','frameHeight']},ensure_ascii=False))
    shutil.copy2(pack/'catalog.json',destination/'catalog.json')
    manifest['localResourcePack']={'entries':len(catalog['entries']),'redistributable':False}
    for path in destination.rglob('*'):
        if path.is_file():manifest['files'][str(path.relative_to(runtime))]=hashlib.sha256(path.read_bytes()).hexdigest()
    manifest_path.write_text(json.dumps(manifest,indent=2)+'\n')
    print('Installed local resources:',len(catalog['entries']))


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--pack',required=True,type=Path)
    parser.add_argument('--runtime',required=True,type=Path)
    parser.add_argument('--native-store',type=Path,action='append',default=[])
    args=parser.parse_args()
    install(args.pack.resolve(),args.runtime.resolve(),[p.resolve() for p in args.native_store])
