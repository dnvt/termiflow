# Homebrew formula for TermiFlow.
#
# Lives in a separate tap repo: github.com/dnvt/homebrew-termiflow
# Path in that repo: Formula/termiflow.rb
#
# Install:
#   brew install dnvt/termiflow/termiflow
#
# After each release:
# 1. Download the four release tarballs
# 2. Run: sha256sum termiflow-v*.tar.gz
# 3. Update the sha256 fields below
# 4. Bump `version` to match the new tag

class Termiflow < Formula
  desc "Terminal-native Mermaid flowchart renderer — jq for diagrams"
  homepage "https://github.com/dnvt/termiflow"
  version "0.1.0"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/dnvt/termiflow/releases/download/v#{version}/termiflow-v#{version}-aarch64-apple-darwin.tar.gz"
      sha256 "b06de3dfb424b0a5b54d1fd391f2912a2b3e0e3960888ddd4cc83c7934c012e0"
    end
    on_intel do
      url "https://github.com/dnvt/termiflow/releases/download/v#{version}/termiflow-v#{version}-x86_64-apple-darwin.tar.gz"
      sha256 "8cfb7e679b5527e03ea6b040b4dd5a6277904daa65af945769eaa69a42a9af8a"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/dnvt/termiflow/releases/download/v#{version}/termiflow-v#{version}-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "f97dafce21c347b65239e971d2bdce94615f73226f16c896feda1479e26858f0"
    end
    on_intel do
      url "https://github.com/dnvt/termiflow/releases/download/v#{version}/termiflow-v#{version}-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "1c305561f3d7a5a842b6fd000efe73c88ef630b5ade55ebc9cc87b52cc0fbf7e"
    end
  end

  def install
    bin.install "termiflow"
    bin.install "tw"
  end

  test do
    assert_match "termiflow", shell_output("#{bin}/tw --help")
  end
end
