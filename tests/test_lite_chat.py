"""Exercise Lite Coder request preparation without contacting a provider."""
import importlib.util
import json
import os
from pathlib import Path
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import Mock, patch


MODULE_PATH = Path(__file__).resolve().parents[1] / 'scripts' / 'serve_lite.py'
spec = importlib.util.spec_from_file_location('spiral_coder_lite_chat_under_test', MODULE_PATH)
lite = importlib.util.module_from_spec(spec)
spec.loader.exec_module(lite)


class LiteChatTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='spiral-lite-{workspace}-')
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name).resolve()
        (self.root / 'project-{tool_root}').mkdir()
        self.workspace = patch.object(lite, 'WORKSPACE_ROOT', self.root)
        self.workspace.start()
        self.addCleanup(self.workspace.stop)
        self.environment = patch.dict(os.environ, {}, clear=True)
        self.environment.start()
        self.addCleanup(self.environment.stop)
        self.reply = 'ローカル模擬応答を受け取りました。'
        self.provider = patch.object(lite, '_http_json', return_value={
            'choices': [{'message': {'role': 'assistant', 'content': self.reply}}],
        })
        self.transport = self.provider.start()
        self.addCleanup(self.provider.stop)
        self.request = {
            'provider': 'openai-compatible',
            'base_url': 'http://127.0.0.1:1/v1',
            'model': 'gpt-4.1-mini',
            'mode': '壁打ち',
            'lang': 'ja',
            'input': 'Lite UI QA: ローカル模擬応答',
            'tool_root': 'project-{tool_root}',
            'force_tools': False,
        }

    def assert_provider_prompt(self):
        self.transport.assert_called_once()
        call = self.transport.call_args
        self.assertEqual(call.args[:2], ('POST', 'http://127.0.0.1:1/v1/chat/completions'))
        payload = call.kwargs['body']
        self.assertEqual(payload['model'], 'gpt-4.1-mini')
        self.assertTrue(payload['tools'], 'normal Coder flow should still expose local tools')
        prompt = payload['messages'][0]['content']
        self.assertIn(f'Workspace root: {self.root.as_posix()}\n', prompt)
        self.assertIn('Tool root (if set): project-{tool_root}\n', prompt)
        examples = [json.loads(line) for line in prompt.splitlines() if line.startswith('{"name":')]
        self.assertEqual([example['name'] for example in examples], [
            'run_command', 'run_command', 'run_command', 'mkdir', 'write_file',
        ])
        self.assertTrue(all(isinstance(example['arguments'], dict) for example in examples))

    def test_coder_chat_reaches_provider_with_literal_json_tool_examples(self):
        self.assertEqual(lite._chat_impl(self.request), {
            'content': self.reply, 'model': 'gpt-4.1-mini',
        })
        self.assert_provider_prompt()

    def test_streaming_coder_flow_emits_response_and_done_instead_of_template_error(self):
        events = []
        handler = SimpleNamespace(
            send_response=Mock(), send_header=Mock(), end_headers=Mock(),
            _write_sse=lambda event, data: events.append((event, data)),
        )
        lite.LiteHandler._serve_chat_stream(handler, self.request)
        self.assertEqual(''.join(data['delta'] for event, data in events if event == 'delta'), self.reply)
        self.assertEqual(events[-1], ('done', {}))
        self.assertFalse(any(event == 'error' for event, _ in events))
        self.assert_provider_prompt()


if __name__ == '__main__':
    unittest.main()
