"""Check the documented fixture result against the actual candidate command."""
import json,os,re,subprocess,tempfile,unittest
from pathlib import Path
from scripts.status_fixture import make,expected
class Guide(unittest.TestCase):
    def test_documented_example(self):
        path=Path('docs/coupled-status.md')
        self.assertTrue(path.is_file(),'coupled-status guide missing')
        text=path.read_text()
        blocks=re.findall(r'```json\s*\n(.*?)\n```',text,re.S)
        self.assertEqual(len(blocks),1,'guide needs one measured JSON block')
        shown=json.loads(blocks[0]); self.assertEqual(shown,expected())
        subprocess.run(['cargo','build','--locked','--quiet','--bin','spiral-coder'],check=True)
        binary=Path(os.environ['CARGO_TARGET_DIR'])/'debug/spiral-coder'
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp); make(root)
            p=subprocess.run([binary,'coupled-status','--run-dir',root],capture_output=True,text=True)
            self.assertEqual(p.returncode,0,p.stderr); self.assertEqual(json.loads(p.stdout),shown)
        for word in ['coupled-status','--run-dir','null','wire','task','verification']:
            self.assertIn(word,text)
        self.assertIn('coupled-status',Path('docs/state-schema.md').read_text())
if __name__=='__main__': unittest.main(verbosity=2)
