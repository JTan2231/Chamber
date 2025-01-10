#!/bin/bash

# Save the original home directory
ORIGINAL_HOME=$HOME

# Create a temporary directory
TEMP_HOME=$(mktemp -d)

# Set HOME to the temp directory
export HOME=$TEMP_HOME

# Run your program here
./../../target/debug/william

# Restore the original HOME
export HOME=$ORIGINAL_HOME

# Clean up the temp directory
rm -rf $TEMP_HOME
