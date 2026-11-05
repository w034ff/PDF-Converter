#!/usr/bin/env bash
# veraPDF 1.28.2 (GPLv3+ / MPLv2+) from Maven Central: the GitHub releases and
# software.verapdf.org are not reachable from the cloud sandbox, Maven Central is.
# usage: scripts/verapdf.sh FILE.pdf...   (checks PDF/A-2b, prints the summary)
set -euo pipefail
V=${VERAPDF_DIR:-$(dirname "$0")/../data/verapdf}
if [[ ! -d "$V/lib" ]]; then
  mkdir -p "$V"
  cat > "$V/pom.xml" <<'POM'
<project xmlns="http://maven.apache.org/POM/4.0.0"><modelVersion>4.0.0</modelVersion>
<groupId>x</groupId><artifactId>vp</artifactId><version>1</version>
<dependencies><dependency><groupId>org.verapdf.apps</groupId><artifactId>greenfield-apps</artifactId><version>1.28.2</version></dependency></dependencies></project>
POM
  (cd "$V" && mvn -q dependency:copy-dependencies -DoutputDirectory=lib)
fi
java -cp "$V/lib/*" org.verapdf.apps.GreenfieldCliWrapper --flavour 2b --format text "$@" 2>/dev/null
