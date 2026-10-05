"""Official Sparkle tooling and a headless native appcast probe; no GUI install."""
import base64, http.server, json, os, pathlib, plistlib, shutil, subprocess, sys, tempfile, threading, unittest, uuid
ROOT=pathlib.Path(__file__).resolve().parents[1]
TOOLS=ROOT/'vendor/bin'
BUNDLE=os.environ.get('TASKBAR_TEST_BUNDLE')
@unittest.skipUnless(sys.platform=='darwin' and (TOOLS/'sign_update').exists(),'Build once to fetch the pinned official Sparkle tools')
class SparkleTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.temp=tempfile.TemporaryDirectory(prefix='taskbar-sparkle-test-')
        cls.root=pathlib.Path(cls.temp.name)
        script=cls.root/'keys.swift'
        script.write_text('''import CryptoKit
import Foundation
let key=Curve25519.Signing.PrivateKey()
let path=CommandLine.arguments[1]
FileManager.default.createFile(atPath:path,contents:key.rawRepresentation.base64EncodedData(),attributes:[.posixPermissions:0o600])
print(key.publicKey.rawRepresentation.base64EncodedString())
''')
        cls.key=cls.root/'seed'
        cls.public=subprocess.run(['xcrun','swift',str(script),str(cls.key)],check=True,capture_output=True,text=True).stdout.strip()
    @classmethod
    def tearDownClass(cls):cls.temp.cleanup()
    def tool(self,*args):
        return subprocess.run([str(TOOLS/'sign_update'),'--ed-key-file',str(self.key),*map(str,args)],capture_output=True,text=True,timeout=20)
    def test_signed_archive_verifies_and_tampering_is_rejected(self):
        archive=self.root/'archive.zip';archive.write_bytes(b'fixture archive, never a published release')
        signed=self.tool('-p',archive);self.assertEqual(signed.returncode,0,signed.stderr)
        signature=signed.stdout.strip();self.assertEqual(self.tool('--verify',archive,signature).returncode,0)
        archive.write_bytes(archive.read_bytes()+b'changed')
        self.assertNotEqual(self.tool('--verify',archive,signature).returncode,0)
    def test_release_configuration_rejects_http_and_malformed_public_keys(self):
        path=self.root/'Info.plist'
        for feed,key in [('http://example.com/feed.xml',self.public),('https://example.com/feed.xml','invalid'),('https://user@example.com/feed.xml',self.public)]:
            path.write_bytes(plistlib.dumps({}))
            env=dict(os.environ,TASKBAR_UPDATE_FEED_URL=feed,TASKBAR_UPDATE_PUBLIC_KEY=key)
            result=subprocess.run([sys.executable,str(ROOT/'scripts/configure-updates.py'),str(path)],env=env,capture_output=True)
            self.assertNotEqual(result.returncode,0)
    def test_signed_appcast_verifies_and_tampering_is_rejected(self):
        feed=self.root/'signed.xml';feed.write_text('<?xml version="1.0"?><rss version="2.0"><channel><title>test</title></channel></rss>')
        signed=self.tool(feed);self.assertEqual(signed.returncode,0,signed.stderr)
        self.assertEqual(self.tool('--verify',feed).returncode,0)
        feed.write_text(feed.read_text().replace('<title>test</title>','<title>tampered</title>'))
        self.assertNotEqual(self.tool('--verify',feed).returncode,0)
    @unittest.skipUnless(BUNDLE,'Set TASKBAR_TEST_BUNDLE to exercise the packaged native updater')
    def test_native_sparkle_reads_local_appcast_without_windows_or_installation(self):
        folder=self.root/'feed';folder.mkdir(exist_ok=True)
        class Quiet(http.server.SimpleHTTPRequestHandler):
            def __init__(self,*a,**kw):super().__init__(*a,directory=str(folder),**kw)
            def log_message(self,*a):pass
        server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Quiet)
        thread=threading.Thread(target=server.serve_forever,daemon=True);thread.start()
        domain='io.sharif.taskbarrust.probe.'+uuid.uuid4().hex
        try:
            app=self.root/'probe/Rowla.app';app.parent.mkdir(exist_ok=True)
            subprocess.run(['ditto',BUNDLE,str(app)],check=True)
            info_path=app/'Contents/Info.plist';info=plistlib.loads(info_path.read_bytes())
            next_version=int(info['CFBundleVersion'])+1
            port=server.server_port
            feed=folder/'appcast.xml'
            feed.write_text(f'''<?xml version="1.0"?><rss version="2.0" xmlns:sparkle="http://www.andymatuschak.org/xml-namespaces/sparkle"><channel><title>Taskbar test</title><item><title>Test update</title><sparkle:version>{next_version}</sparkle:version><sparkle:shortVersionString>0.1.1</sparkle:shortVersionString><enclosure url="http://127.0.0.1:{port}/unused.zip" length="1" type="application/octet-stream" sparkle:edSignature="{'A'*86}==" /></item></channel></rss>''')
            signed=self.tool(feed);self.assertEqual(signed.returncode,0,signed.stderr)
            info.update(SUFeedURL=f'http://127.0.0.1:{port}/appcast.xml',SUPublicEDKey=self.public,SUEnableAutomaticChecks=False,SUAllowsAutomaticUpdates=False,NSAppTransportSecurity={'NSAllowsLocalNetworking':True})
            info_path.write_bytes(plistlib.dumps(info))
            subprocess.run([str(ROOT/'scripts/sign-bundle.sh'),str(app)],check=True,capture_output=True)
            unsafe_probe=subprocess.run([str(app/'Contents/MacOS/taskbar-rs'),'--probe-updater'],capture_output=True,text=True,timeout=30)
            self.assertNotEqual(unsafe_probe.returncode,0)
            self.assertIn('isolated test defaults domain',unsafe_probe.stdout)
            info['SUDefaultsDomain']=domain
            info_path.write_bytes(plistlib.dumps(info))
            subprocess.run([str(ROOT/'scripts/sign-bundle.sh'),str(app)],check=True,capture_output=True)
            result=subprocess.run([str(app/'Contents/MacOS/taskbar-rs'),'--probe-updater'],capture_output=True,text=True,timeout=30)
            self.assertEqual(result.returncode,0,result.stdout+result.stderr)
            data=json.loads(result.stdout.strip().splitlines()[-1]);self.assertTrue(data['finished']);self.assertEqual(data['version'],str(next_version));self.assertIsNone(data['error'])
            # The native framework, not just the command-line verifier, must
            # reject a changed feed before presenting any update information.
            feed.write_text(feed.read_text().replace('<title>Test update</title>','<title>Tampered update</title>'))
            rejected=subprocess.run([str(app/'Contents/MacOS/taskbar-rs'),'--probe-updater'],capture_output=True,text=True,timeout=30)
            self.assertNotEqual(rejected.returncode,0,rejected.stdout+rejected.stderr)
            data=json.loads(rejected.stdout.strip().splitlines()[-1]);self.assertTrue(data['finished']);self.assertIsNone(data['version']);self.assertIsNotNone(data['error'])
        finally:
            server.shutdown();server.server_close();thread.join(timeout=5)
            subprocess.run(['/usr/bin/defaults','delete',domain],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
