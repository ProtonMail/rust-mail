#!/bin/bash

# Get the directory where this script is located
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CHECK_SCRIPT="$SCRIPT_DIR/check_migration_filenames.sh"

# Function to check multiple directories
check_multiple_directories() {
    local directories=("$@")
    local total_dirs=${#directories[@]}
    local successful_dirs=0
    local failed_dirs=()
    
    echo "Checking $total_dirs directories for SQL file naming compliance..."
    echo "=================================================="
    echo
    
    for i in "${!directories[@]}"; do
        local dir="${directories[$i]}"
        local dir_num=$((i + 1))
        
        echo "[$dir_num/$total_dirs] Checking directory: $dir"
        echo "----------------------------------------"
        
        # Run the check_sql_filenames.sh script using the full path
        if "$CHECK_SCRIPT" "$dir"; then
            echo "✅ Directory '$dir' passed all checks"
            ((successful_dirs++))
        else
            echo "❌ Directory '$dir' failed checks"
            failed_dirs+=("$dir")
        fi
        
        echo
        echo "=================================================="
        echo
    done
    
    # Final summary
    echo "🏁 FINAL SUMMARY"
    echo "================"
    echo "Total directories checked: $total_dirs"
    echo "Successful directories: $successful_dirs"
    echo "Failed directories: ${#failed_dirs[@]}"
    
    if [[ ${#failed_dirs[@]} -gt 0 ]]; then
        echo
        echo "Directories that failed:"
        for dir in "${failed_dirs[@]}"; do
            echo "  - $dir"
        done
        echo
        echo "❌ Overall result: FAILED"
        exit 1
    else
        echo
        echo "🎉 Overall result: ALL PASSED"
        exit 0
    fi
}

# Main execution
if [[ $# -eq 0 ]]; then
    echo "Usage: $0 <directory1> [directory2] [directory3] ..."
    echo "Example: $0 /path/to/sql1 /path/to/sql2 ./local/sql"
    exit 1
fi

# Check if check_sql_filenames.sh exists and is executable
if [[ ! -f "$CHECK_SCRIPT" ]]; then
    echo "Error: check_sql_filenames.sh not found at $CHECK_SCRIPT"
    exit 1
fi

if [[ ! -x "$CHECK_SCRIPT" ]]; then
    echo "Error: $CHECK_SCRIPT is not executable. Run: chmod +x $CHECK_SCRIPT"
    exit 1
fi

# Pass all arguments to the function
check_multiple_directories "$@"