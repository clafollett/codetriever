#!/bin/bash
# Initialize test repositories with shallow clones to save bandwidth and disk space

echo "Initializing test repositories with shallow clones..."

# Initialize all submodules with depth=1 (shallow)
git submodule update --init --depth 1

echo "Test repositories initialized!"
echo "Saved bandwidth by only fetching the specific commits needed for testing."

# Show sizes
echo -e "\nRepository sizes:"
du -sh */