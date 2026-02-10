use lightningcss::{
  declaration::DeclarationBlock,
  printer::{Printer, PrinterOptions},
  stylesheet::ParserOptions,
  traits::ToCss,
};
use std::sync::{Arc, RwLock};

// Helper function to convert DeclarationBlock to string
fn declaration_block_to_string(decl_block: &DeclarationBlock) -> Result<String, lightningcss::error::PrinterError> {
    let mut output = String::new();
    let options = PrinterOptions::default();
    let mut printer = Printer::new(&mut output, options);
    decl_block.to_css(&mut printer)?;
    Ok(output)
}

#[test]
fn test_error_recovery_invalid_declaration() {
    // Test case for the reported bug: "*{color: red;important;background: yellow;}"
    let css_input = "color: red;important;background: yellow;";
    
    // Test with error_recovery: false - should fail
    let options_strict = ParserOptions {
        error_recovery: false,
        ..Default::default()
    };
    
    let result_strict = DeclarationBlock::parse_string(css_input, options_strict);
    println!("Strict parsing result: {:?}", result_strict);
    
    // We expect this to fail because "important" is not a valid property (missing colon)
    assert!(result_strict.is_err(), "Strict parsing should fail on invalid 'important;' declaration");
    
    // Test with error_recovery: true - should succeed and preserve valid declarations
    let warnings = Arc::new(RwLock::new(Vec::new()));
    let options_recovery = ParserOptions {
        error_recovery: true,
        warnings: Some(warnings.clone()),
        ..Default::default()
    };
    
    let result_recovery = DeclarationBlock::parse_string(css_input, options_recovery);
    println!("Recovery parsing result: {:?}", result_recovery);
    
    match result_recovery {
        Ok(decl_block) => {
            println!("Parsed declarations: {:?}", decl_block.declarations);
            println!("Parsed important declarations: {:?}", decl_block.important_declarations);
            
            // Convert to CSS to see what we actually got
            let css_output = declaration_block_to_string(&decl_block).unwrap();
            println!("CSS output: '{}'", css_output);
            
            // Expected behavior: should contain both "color: red" and "background: yellow"
            // but NOT the invalid "important" declaration
            // Note: yellow may be represented as hex (#ff0) so we check for "background:"
            assert!(css_output.contains("color: red"), "Should preserve valid 'color: red' declaration");
            assert!(css_output.contains("background:"), "Should preserve 'background' declaration (color representation may vary)");
            assert!(!css_output.contains("important"), "Should NOT contain invalid 'important' declaration");
            
            // Check that we have exactly 2 declarations (color and background)
            let total_declarations = decl_block.declarations.len() + decl_block.important_declarations.len();
            assert_eq!(total_declarations, 2, "Should have exactly 2 valid declarations after error recovery");
            
        },
        Err(e) => {
            panic!("Error recovery parsing should succeed but got error: {:?}", e);
        }
    }
    
    // Check that warnings were generated
    let warnings_vec = warnings.read().unwrap();
    println!("Warnings generated: {:?}", *warnings_vec);
    assert!(!warnings_vec.is_empty(), "Should have generated warnings for invalid 'important;' declaration");
}

#[test]
fn test_error_recovery_missing_colon() {
    // Test various cases where property name is followed by semicolon instead of colon
    let test_cases = vec![
        ("color: red; invalid; background: blue;", 2), // should preserve color and background
        ("margin: 10px; broken_prop; padding: 5px;", 2), // should preserve margin and padding
        ("display: block; another_invalid; visibility: hidden;", 2), // should preserve display and visibility
    ];
    
    for (css_input, expected_count) in test_cases {
        println!("\nTesting: '{}'", css_input);
        
        let warnings = Arc::new(RwLock::new(Vec::new()));
        let options = ParserOptions {
            error_recovery: true,
            warnings: Some(warnings.clone()),
            ..Default::default()
        };
        
        let result = DeclarationBlock::parse_string(css_input, options);
        match result {
            Ok(decl_block) => {
                let total_declarations = decl_block.declarations.len() + decl_block.important_declarations.len();
                println!("Got {} declarations for input: '{}'", total_declarations, css_input);
                
                let css_output = declaration_block_to_string(&decl_block).unwrap();
                println!("CSS output: '{}'", css_output);
                
                assert_eq!(total_declarations, expected_count, 
                    "Expected {} declarations but got {} for input: '{}'", 
                    expected_count, total_declarations, css_input);
            },
            Err(e) => {
                panic!("Error recovery should succeed for '{}' but got error: {:?}", css_input, e);
            }
        }
        
        let warnings_vec = warnings.read().unwrap();
        println!("Warnings for '{}': {:?}", css_input, *warnings_vec);
    }
}

#[test]
fn test_error_recovery_comprehensive() {
    // Test the exact case from the problem statement
    let style_rule_css = "*{color: red;important;background: yellow;}";
    
    println!("Testing style rule: '{}'", style_rule_css);
    
    // Parse the entire style rule
    let warnings = Arc::new(RwLock::new(Vec::new()));
    let options = ParserOptions {
        error_recovery: true,
        warnings: Some(warnings.clone()),
        ..Default::default()
    };
    
    let result = lightningcss::stylesheet::StyleSheet::parse(style_rule_css, options);
    match result {
        Ok(stylesheet) => {
            println!("Successfully parsed stylesheet: {:?}", stylesheet);
            
            // Convert back to CSS
            let printer_options = PrinterOptions::default();
            let css_output = stylesheet.to_css(printer_options).unwrap();
            println!("Full CSS output: '{}'", css_output.code);
            
            // Check that the valid properties are preserved
            assert!(css_output.code.contains("color: red"), "Should preserve 'color: red' in full stylesheet");
            assert!(css_output.code.contains("background:"), "Should preserve 'background' property in full stylesheet (color representation may vary)");
            assert!(!css_output.code.contains("important") || css_output.code.contains("!important"), 
                "Should not contain bare 'important' (only '!important' is valid)");
        },
        Err(e) => {
            panic!("Full stylesheet parsing with error recovery should succeed but got: {:?}", e);
        }
    }
    
    let warnings_vec = warnings.read().unwrap();
    println!("Warnings from full stylesheet parsing: {:?}", *warnings_vec);
}

#[test]
fn test_error_recovery_edge_cases() {
    // Test additional edge cases to ensure robustness
    let edge_cases = vec![
        // Multiple consecutive invalid declarations
        ("color: red; invalid1; invalid2; background: blue;", 2),
        // Invalid at start
        ("invalid_start; color: red; background: blue;", 2),
        // Invalid at end
        ("color: red; background: blue; invalid_end;", 2),
        // Only invalid declarations
        ("invalid1; invalid2; invalid3;", 0),
        // Mixed with valid important declarations
        ("color: red !important; invalid; background: blue;", 2),
        // Empty values (actually valid, just empty)
        ("color:; background: yellow;", 2),  // Empty value is actually parsed as valid
    ];
    
    for (css_input, expected_count) in edge_cases {
        println!("\nTesting edge case: '{}'", css_input);
        
        let warnings = Arc::new(RwLock::new(Vec::new()));
        let options = ParserOptions {
            error_recovery: true,
            warnings: Some(warnings.clone()),
            ..Default::default()
        };
        
        let result = DeclarationBlock::parse_string(css_input, options);
        match result {
            Ok(decl_block) => {
                let total_declarations = decl_block.declarations.len() + decl_block.important_declarations.len();
                println!("Got {} declarations for edge case: '{}'", total_declarations, css_input);
                
                let css_output = declaration_block_to_string(&decl_block).unwrap();
                println!("CSS output: '{}'", css_output);
                
                assert_eq!(total_declarations, expected_count, 
                    "Expected {} declarations but got {} for edge case: '{}'", 
                    expected_count, total_declarations, css_input);
            },
            Err(e) => {
                if expected_count > 0 {
                    panic!("Error recovery should succeed for '{}' but got error: {:?}", css_input, e);
                } else {
                    println!("Expected failure for edge case with no valid declarations: '{}'", css_input);
                }
            }
        }
        
        let warnings_vec = warnings.read().unwrap();
        println!("Warnings for edge case '{}': {:?}", css_input, *warnings_vec);
    }
}

#[test] 
fn test_error_recovery_disabled() {
    // Ensure that when error_recovery is false, parsing fails immediately
    let css_input = "color: red;important;background: yellow;";
    
    let options_strict1 = ParserOptions {
        error_recovery: false,
        ..Default::default()
    };
    
    let result_strict = DeclarationBlock::parse_string(css_input, options_strict1);
    assert!(result_strict.is_err(), "Should fail when error_recovery is disabled");
    
    // Test with full stylesheet parsing too
    let style_rule_css = "*{color: red;important;background: yellow;}";
    let options_strict2 = ParserOptions {
        error_recovery: false,
        ..Default::default()
    };
    let result_stylesheet = lightningcss::stylesheet::StyleSheet::parse(style_rule_css, options_strict2);
    assert!(result_stylesheet.is_err(), "Full stylesheet should fail when error_recovery is disabled");
}