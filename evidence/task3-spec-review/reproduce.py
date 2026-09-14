from pathlib import Path
import tempfile, subprocess
here=Path(__file__).resolve().parent
product=here.parents[1]/"product"
out=Path(tempfile.mkdtemp(prefix="task3-spec-reproduce-"))
for case,source in [("observer-capacity","observer_capacity.rs"),("socket-deadline-config","review.rs"),("usage","review.rs")]:
    command=["rustc","+1.88.0","--edition=2024",str(here/case/source),"-o",str(out/case)]
    if case!="observer-capacity":
        command += ["--test","-L","dependency="+str(product/"target/debug/deps")]
        for name in ["llmgw","tokio","serde_json","socket2"]:
            lib=max((product/"target/debug/deps").glob("lib"+name+"-*.rlib"),key=lambda p:p.stat().st_mtime)
            command += ["--extern",name+"="+str(lib)]
    subprocess.run(command,check=True)
    command=[str(out/case)]
    if case!="observer-capacity": command += ["review_","--nocapture"]
    result=subprocess.run(command,check=False)
    print(case,"exit",result.returncode,flush=True)
