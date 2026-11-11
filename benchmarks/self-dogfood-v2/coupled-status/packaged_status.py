"""Exercise the assembled npm bin with the independently built candidate."""
import hashlib,json,re,shutil,subprocess,sys,tempfile,unittest
from pathlib import Path
sys.path.insert(0,str(Path.cwd()))
from scripts.status_fixture import make,expected
BINARY=Path(sys.argv.pop()).resolve()
class Package(unittest.TestCase):
    def test_route(self):
        help_text=subprocess.check_output([BINARY,'--help'],text=True)
        self.assertRegex(help_text,r'(?m)^\s+coupled-status(?:\s|$)','coupled-status command missing from --help')
        host=re.search(r'^host: (\S+)$',subprocess.check_output(['rustc','-vV'],text=True),re.M)[1]
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp); artifact=root/'input'/host/BINARY.name; artifact.parent.mkdir(parents=True)
            shutil.copyfile(BINARY,artifact); package=root/'package'
            subprocess.run(['node','scripts/prepare-npm.mjs','--input',root/'input','--out',package,'--local-target',host],check=True)
            manifest=json.loads((package/'native-manifest.json').read_text())
            self.assertEqual(manifest['binaries'][host]['sha256'],hashlib.sha256(BINARY.read_bytes()).hexdigest())
            entry=package/json.loads((package/'package.json').read_text())['bin']['spiral']
            run=root/'run'; make(run,'interrupted')
            p=subprocess.run(['node',entry,'coupled-status','--run-dir',run],capture_output=True,text=True,timeout=20)
            self.assertEqual(p.returncode,0,p.stderr); self.assertEqual(json.loads(p.stdout),expected('interrupted'))
if __name__=='__main__': unittest.main(verbosity=2)
