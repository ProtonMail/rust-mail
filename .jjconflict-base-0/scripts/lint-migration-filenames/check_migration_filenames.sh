#!/bin/bash

# Function to check filenames in a directory
check_migration_filenames() {
    local directory_path="${1:-.}"  # Use current directory if no path provided

    # Check if directory exists
    if [[ ! -d "$directory_path" ]]; then
        echo "Error: Directory '$directory_path' not found."
        exit 1
    fi

    # Regex pattern for the required format (any extension)
    local pattern='^v[0-9]{3}_[a-zA-Z0-9]+(_[a-zA-Z0-9]+)*\.[a-zA-Z0-9]+$'

    # Get all files (not directories) in the directory
    local files=()
    for file in "$directory_path"/*; do
        if [[ -f "$file" ]]; then
            files+=("$(basename "$file")")
        fi
    done

    if [[ ${#files[@]} -eq 0 ]]; then
        echo "No files found in '$directory_path'"
        exit 0
    fi

    echo "Checking ${#files[@]} files in '$directory_path':"
    echo

    local conforming_count=0
    local non_conforming_files=()

    for filename in "${files[@]}"; do
        if [[ $filename =~ $pattern ]]; then
            echo "✓ $filename"
            ((conforming_count++))
        else
            echo "✗ $filename"
            non_conforming_files+=("$filename")
        fi
    done

    echo
    echo "--- Summary ---"
    echo "Total files: ${#files[@]}"
    echo "Conforming files: $conforming_count"
    echo "Non-conforming files: ${#non_conforming_files[@]}"

    if [[ ${#non_conforming_files[@]} -gt 0 ]]; then
        echo
        echo "Files that don't conform to pattern:"
        for file in "${non_conforming_files[@]}"; do
            echo "  - $file"
        done
        exit 1
    else
        echo
        echo "🎉 All files conform to the naming pattern!"
        exit 0
    fi
}

# Main execution
if [[ $# -eq 0 ]]; then
    echo "Usage: $0 <directory_path>"
    echo "Or run without arguments to check current directory"
    read -p "Enter directory path (or press Enter for current directory): " dir_path
    check_migration_filenames "$dir_path"
else
    check_migration_filenames "$1"
fi