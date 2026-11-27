#!/bin/sh
# Removes walkie so install.tape records a real fresh install.
# Keeps the shared Playwright browsers and your walkie library.
set -x
claude plugin uninstall walkie@nijicha-skills
claude plugin marketplace remove nijicha-skills
brew uninstall walkie
rm -rf "$HOME/.cache/walkie"
