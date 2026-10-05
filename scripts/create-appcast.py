#!/usr/bin/env python3
"""Generate a feed for one notarized archive. Signing happens in release.sh."""
import pathlib
import plistlib
import subprocess
import sys
import urllib.parse
import xml.etree.ElementTree as ET


def generate(app, archive, url, output, signature):
    parsed = urllib.parse.urlsplit(url)
    if (parsed.scheme != "https" or not parsed.hostname or parsed.username
            or parsed.password or parsed.fragment):
        raise ValueError("Release URLs require HTTPS without credentials or fragments")
    info = plistlib.loads((app / "Contents/Info.plist").read_bytes())
    parts = str(info["LSMinimumSystemVersion"]).split(".")
    if len(parts) > 3 or any(not part.isdigit() for part in parts):
        raise ValueError("Minimum macOS version must contain one to three integers")
    minimum_os = ".".join(parts + ["0"] * (3 - len(parts)))
    architectures = set(subprocess.check_output(
        ["lipo", "-archs", str(app / "Contents/MacOS/taskbar-rs")], text=True
    ).split())
    if architectures not in ({"arm64"}, {"arm64", "x86_64"}):
        raise ValueError("Public releases require Apple Silicon or a universal binary")
    namespace = "http://www.andymatuschak.org/xml-namespaces/sparkle"
    ET.register_namespace("sparkle", namespace)
    rss = ET.Element("rss", version="2.0")
    channel = ET.SubElement(rss, "channel")
    ET.SubElement(channel, "title").text = "Rowla"
    item = ET.SubElement(channel, "item")
    ET.SubElement(item, "title").text = "Rowla " + info["CFBundleShortVersionString"]
    fields = {
        "version": info["CFBundleVersion"],
        "shortVersionString": info["CFBundleShortVersionString"],
        "minimumSystemVersion": minimum_os,
    }
    if architectures == {"arm64"}:
        fields["hardwareRequirements"] = "arm64"
    for key, value in fields.items():
        ET.SubElement(item, f"{{{namespace}}}{key}").text = str(value)
    ET.SubElement(item, "enclosure", url=url, length=str(archive.stat().st_size),
                  type="application/octet-stream",
                  attrib={f"{{{namespace}}}edSignature": signature})
    ET.indent(rss)
    ET.ElementTree(rss).write(output, encoding="utf-8", xml_declaration=True)


if __name__ == "__main__":
    try:
        generate(pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2]), sys.argv[3],
                 pathlib.Path(sys.argv[4]), sys.argv[5])
    except (ValueError, IndexError) as error:
        raise SystemExit(str(error))
