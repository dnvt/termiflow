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
      sha256 "PLACEHOLDER_aarch64_apple_darwin"
    end
    on_intel do
      url "https://github.com/dnvt/termiflow/releases/download/v#{version}/termiflow-v#{version}-x86_64-apple-darwin.tar.gz"
      sha256 "PLACEHOLDER_x86_64_apple_darwin"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/dnvt/termiflow/releases/download/v#{version}/termiflow-v#{version}-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "PLACEHOLDER_aarch64_unknown_linux_gnu"
    end
    on_intel do
      url "https://github.com/dnvt/termiflow/releases/download/v#{version}/termiflow-v#{version}-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "PLACEHOLDER_x86_64_unknown_linux_gnu"
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
