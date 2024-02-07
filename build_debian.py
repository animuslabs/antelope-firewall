#!/usr/bin/python3
import subprocess
import os
from os import path
import tomllib
import shutil

with open("./antelope-firewall/Cargo.toml", 'rb') as f:
    data = tomllib.load(f)

subprocess.run(["cargo", "build", "--release"])

version = data["package"]["version"]

build_base = f"antelope-firewall_{version}-1"

os.makedirs(build_base, exist_ok=True)
os.makedirs(path.join(build_base, "DEBIAN"), exist_ok=True)

os.makedirs(path.join(build_base, "usr/bin"), exist_ok=True)
shutil.copyfile("target/release/antelope-firewall", path.join(build_base, "usr/bin/antelope-firewall"))
subprocess.run(["chmod", "+x", path.join(build_base, "usr/bin/antelope-firewall")])

os.makedirs(path.join(build_base, "etc/antelope-firewall"), exist_ok=True)
shutil.copyfile("config_main.toml", path.join(build_base, "etc/antelope-firewall/config.toml"))

with open(path.join(build_base, "DEBIAN", "control"), "w") as f:
    f.write(f"""Package: antelope-firewall
Version: {version}-1
Section: base
Priority: optional
Architecture: amd64
Depends:
Maintainer: Matthew Jurenka <main@matthewjurenka.com>
Description: Firewall and Rate Limiter and Load Balancer
 for Antelope blockchain RPCs. For more information:
 https://github.com/animuslabs/antelope-firewall
""")

os.makedirs(path.join(build_base, "etc/systemd/system"), exist_ok=True)
with open(path.join(build_base, "etc/systemd/system/antelope-firewall.service"), "w") as f:
    f.write(f"""[Unit]
Description=Firewall/Rate Limiter/Load Balancer for Antelope RPC Nodes

[Service]
ExecStart=/usr/bin/antelope-firewall

[Install]
WantedBy=multi-user.target
""")

subprocess.run(["dpkg-deb", "--build", build_base])
