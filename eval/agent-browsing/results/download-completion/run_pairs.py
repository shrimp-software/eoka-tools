import json
from pathlib import Path
import subprocess

root = Path('/var/tmp/eoka-agent-eval/download-fix')
bins = {
    'before': '/var/tmp/eoka-agent-eval/final-2/bin/eoka',
    'after': str(root / 'candidate/eoka'),
}
results = []
for pair in range(3):
    order = ['before', 'after'] if pair % 2 == 0 else ['after', 'before']
    for version in order:
        output = root / f'{version}-{pair}'
        command = ['python3', 'eval/agent-browsing/run.py', '--eoka', bins[version], '--browser-use', '/var/tmp/eoka-agent-eval/venv/bin/browser-use', '--tasks', 'download', '--backends', 'eoka', '--output', str(output)]
        subprocess.run(command, cwd='/home/cbass/Code/eoka-tools', check=True)
        result = json.loads((output / 'results.json').read_text())[0]
        results.append({'pair': pair, 'version': version, 'directory': str(output), **result})
        (root / 'results.json').write_text(json.dumps(results, indent=2))
        print(json.dumps({'pair': pair, 'version': version, 'calls': result.get('tool_calls'), 'seconds': result.get('elapsed_s'), 'success': result['success'], 'valid': result.get('valid')}), flush=True)
