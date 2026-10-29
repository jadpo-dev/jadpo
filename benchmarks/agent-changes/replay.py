#!/usr/bin/env python3
"""A stand-in agent for checking the harness's plumbing without spending
tokens: it plays a scripted session (edits and commands) in the working
directory and prints it as Claude Code's stream-json, usage zero.

  python3 replay.py <script.json> <prompt>
"""
import json
import subprocess
import sys
from pathlib import Path


def emit(event):
    print(json.dumps(event), flush=True)


def main():
    script = json.loads(Path(sys.argv[1]).read_text())
    usage = {'input_tokens': 0, 'output_tokens': 0, 'cache_creation_input_tokens': 0, 'cache_read_input_tokens': 0}
    emit({'type': 'system', 'subtype': 'init', 'model': 'replay', 'tools': ['Bash', 'Edit']})
    for number, step in enumerate(script['steps']):
        identifier = f'replay_{number}'
        if 'edit' in step:
            name, data = 'Edit', {'file_path': step['edit'], 'old_string': step['old'], 'new_string': step['new']}
            path = Path(step['edit'])
            text = path.read_text()
            error = step['old'] not in text
            if not error:
                path.write_text(text.replace(step['old'], step['new'], 1))
            output = 'old_string not found' if error else 'edited'
        else:
            name, data = 'Bash', {'command': step['bash']}
            done = subprocess.run(step['bash'], shell=True, capture_output=True, text=True)
            error = done.returncode != 0
            output = (f'Exit code {done.returncode}\n' if error else '') + (done.stdout + done.stderr)[-4000:]
        emit({'type': 'assistant', 'message': {'id': identifier, 'usage': usage,
              'content': [{'type': 'tool_use', 'id': identifier, 'name': name, 'input': data}]}})
        emit({'type': 'user', 'message': {'content': [{'type': 'tool_result', 'tool_use_id': identifier,
              'is_error': error, 'content': output}]}})
    emit({'type': 'result', 'subtype': 'success', 'session_id': 'replay', 'total_cost_usd': 0, 'num_turns': len(script['steps']),
          'is_error': False, 'usage': usage, 'modelUsage': {}, 'result': script.get('say', 'done')})


if __name__ == '__main__':
    main()
