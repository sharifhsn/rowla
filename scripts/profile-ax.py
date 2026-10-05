#!/usr/bin/env python3
"""Background-only native AX fixture and command-actor latency profile."""
import argparse,json,pathlib,plistlib,subprocess,tempfile
ROOT=pathlib.Path(__file__).resolve().parents[1]
parser=argparse.ArgumentParser();parser.add_argument('--bundle',type=pathlib.Path,default=ROOT/'dist/Rowla.app');parser.add_argument('--seconds',type=int,default=10);parser.add_argument('--label',default='ax');parser.add_argument('--churn',action='store_true');args=parser.parse_args()
output=ROOT/'diagnostics';output.mkdir(exist_ok=True)
with tempfile.TemporaryDirectory(prefix='taskbar-ax-fixture-') as temp:
    app=pathlib.Path(temp)/'Taskbar AX fixture.app';mac=app/'Contents/MacOS';mac.mkdir(parents=True)
    (app/'Contents/Info.plist').write_bytes(plistlib.dumps(dict(CFBundleIdentifier='io.sharif.taskbarrust.axfixture',CFBundleName='Taskbar AX fixture',CFBundleExecutable='ax-provider',CFBundlePackageType='APPL',LSUIElement=True,NSPrincipalClass='FixtureApplication')))
    exe=mac/'ax-provider'
    subprocess.run(['xcrun','clang','-fobjc-arc','-framework','AppKit',str(ROOT/'tests/fixtures/ax-provider.m'),'-o',str(exe)],check=True,capture_output=True)
    subprocess.run(['codesign','--force','--sign','-',str(app)],check=True,capture_output=True)
    results=[]
    workloads=[('slow-client',250,1,0),('large-window-set',0,64,0)]
    if args.churn:workloads.append(('window-churn',0,64,40))
    for label,delay,count,cycles in workloads:
        fixture=subprocess.Popen([str(exe),str(delay),str(count),str(cycles)],stdout=subprocess.PIPE,stderr=subprocess.DEVNULL,text=True)
        try:
            if fixture.stdout.readline().strip()!='ready':raise RuntimeError('Fixture did not initialize')
            result=subprocess.run([str(args.bundle/'Contents/MacOS/taskbar-rs'),'--benchmark-latency',str(args.seconds),str(fixture.pid)],capture_output=True,text=True,timeout=args.seconds+20)
            (output/f'{args.label}-{label}.json').write_text(result.stdout)
            if result.returncode:raise RuntimeError(result.stdout+result.stderr)
            data=json.loads(result.stdout.strip().splitlines()[-1]);data['fixture']=label;results.append(data)
            if label=='slow-client' and data['discovery']['timeouts']==0:raise RuntimeError('Slow native client did not exercise timeout handling')
        finally:fixture.terminate();fixture.wait(timeout=5)
        if cycles:
            log=fixture.stdout.read();(output/f'{args.label}-{label}-fixture.txt').write_text(log)
            if not log.strip():raise RuntimeError('Churn did not complete within the profile duration')
            summary=json.loads(log.strip().splitlines()[-1]);data['churn']=summary
            if summary['cycles']!=cycles:raise RuntimeError('Churn workload did not finish')
        if label in ('large-window-set','window-churn') and data['fixture_windows']!=count:raise RuntimeError(f'Window set not recovered: {data}')
    print(json.dumps(results,indent=2))
