#!/bin/bash

# Define the list of folders
folders=("bootloader" "conductor" "cortex" "intent" "kernel" "volume")

# Loop through each folder
for folder in "${folders[@]}"; do
    # Check if the directory exists
    if [ -d "$folder" ]; then
        echo "----------------------------------------"
        echo ":rocket: Processing: $folder"
        
        # Run git pull inside the folder (using a subshell so we don't lose our place)
        (cd "$folder" && git pull)
        
        if [ $? -eq 0 ]; then
            echo ":white_check_mark: Successfully pulled $folder"
        else
            echo ":x: Failed to pull $folder"
        fi
    else
        echo ":warning:  Directory '$folder' not found, skipping..."
    fi
done

echo "----------------------------------------"
echo "All done."