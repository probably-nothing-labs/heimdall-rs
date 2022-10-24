use heimdall_common::{ether::evm::types::{byte_size_to_type, find_cast}, utils::strings::{find_balanced_parentheses, find_balanced_parentheses_backwards}};

use crate::decompile::constants::{ENCLOSED_EXPRESSION_REGEX};

use super::{constants::{AND_BITMASK_REGEX, AND_BITMASK_REGEX_2, NON_ZERO_BYTE_REGEX}};

fn convert_bitmask_to_casting(line: String) -> String {
    let mut cleaned = line;

    match AND_BITMASK_REGEX.find(&cleaned) {
        Some(bitmask) => {
            let cast = bitmask.as_str();
            let cast_size = NON_ZERO_BYTE_REGEX.find_iter(&cast).count();
            let (_, cast_types) = byte_size_to_type(cast_size);

            // get the cast subject
            let mut subject = cleaned.get(bitmask.end()..).unwrap().replace(";",  "");
            
            // attempt to find matching parentheses
            let subject_indices = find_balanced_parentheses(subject.to_string());
            subject = match subject_indices.2 {
                true => {

                    // get the subject as hte substring between the balanced parentheses found in unbalanced subject
                    subject[subject_indices.0..subject_indices.1].to_string()
                },
                false => {

                    // this shouldn't happen, but if it does, just return the subject.
                    //TODO add this to verbose logs
                    subject
                },
            };

            // apply the cast to the subject
            cleaned = cleaned.replace(
                &format!("{}{}", cast, subject),
                &format!("{}{}", cast_types[0], subject),
            );

            // attempt to cast again
            cleaned = convert_bitmask_to_casting(cleaned);
        },
        None => {

            match AND_BITMASK_REGEX_2.find(&cleaned) {
                Some(bitmask) => {
                    let cast = bitmask.as_str();
                    let cast_size = NON_ZERO_BYTE_REGEX.find_iter(&cast).count();
                    let (_, cast_types) = byte_size_to_type(cast_size);
        
                    // get the cast subject
                    let mut subject = match cleaned.get(0..bitmask.start()).unwrap().replace(";",  "").split("=").collect::<Vec<&str>>().last() {
                        Some(subject) => subject.to_string(),
                        None => cleaned.get(0..bitmask.start()).unwrap().replace(";",  "").to_string(),
                    };

                    // attempt to find matching parentheses
                    let subject_indices = find_balanced_parentheses_backwards(subject.to_string());

                    subject = match subject_indices.2 {
                        true => {
        
                            // get the subject as hte substring between the balanced parentheses found in unbalanced subject
                            subject[subject_indices.0..subject_indices.1].to_string()
                        },
                        false => {
                            
                            // this shouldn't happen, but if it does, just return the subject.
                            //TODO add this to verbose logs
                            subject
                        },
                    };

                    // apply the cast to the subject
                    cleaned = cleaned.replace(
                        &format!("{}{}", subject, cast),
                        &format!("{}{}", cast_types[0], subject),
                    );
        
                    // attempt to cast again
                    cleaned = convert_bitmask_to_casting(cleaned);
                },
                None => {}
            }
            
        }
    }

    cleaned
}

fn simplify_casts(line: String) -> String {
    let mut cleaned = line;

    // remove unnecessary casts
    let (cast_start, cast_end, cast_type) = find_cast(cleaned.to_string());
    
    match cast_type {
        Some(cast) => {
            let cleaned_cast_pre = cleaned[0..cast_start].to_string();
            let cleaned_cast_post = cleaned[cast_end..].to_string();
            let cleaned_cast = cleaned[cast_start..cast_end].to_string().replace(&cast, "");

            cleaned = format!("{}{}{}", cleaned_cast_pre, cleaned_cast, cleaned_cast_post);

            // check if there are remaining casts
            let (_, _, remaining_cast_type) = find_cast(cleaned_cast_post.clone());
            match remaining_cast_type {
                Some(_) => {

                    // a cast is remaining, simplify it
                    let mut recursive_cleaned = format!("{}{}", cleaned_cast_pre, cleaned_cast);
                    recursive_cleaned.push_str(
                        simplify_casts(cleaned_cast_post).as_str()
                    );
                    cleaned = recursive_cleaned;
                },
                None => {}
            }
        },
        None => {}
    }

    cleaned
}

fn simplify_parentheses(line: String, paren_index: usize) -> String {

    // helper function to determine if parentheses are necessary
    fn are_parentheses_unnecessary(expression: String) -> bool {

        // safely grab the first and last chars
        let first_char = match expression.get(0..1) {
            Some(x) => x,
            None => "",
        };
        let last_char = match expression.get(expression.len() - 1..expression.len()) {
            Some(x) => x,
            None => "",
        };

        // if there is a negation of an expression, remove the parentheses
        // helps with double negation
        if first_char == "!" && last_char == ")" { return true; }

        // remove the parentheses if the expression is within brackets
        if first_char == "[" && last_char == "]" { return true; }

        // parens required if:
        //  - expression is a cast
        //  - expression is a function call
        //  - expression is the surrounding parens of a conditional
        if first_char != "(" { return false; }
        else if last_char == ")" { return true; }

        // don't include instantiations
        if expression.contains("memory ret") { return false; }

        // handle the inside of the expression
        let inside = match expression.get(2..expression.len() - 2) {
            Some(x) => {
                ENCLOSED_EXPRESSION_REGEX
                    .replace(x, "x").to_string()
            },
            None => "".to_string(),
        };

        if inside.len() > 0 {
            let expression_parts = inside.split(|x| ['*', '/', '=', '>', '<', '|', '&', '!']
                .contains(&x))
                .filter(|x| x.len() > 0).collect::<Vec<&str>>();    

            return expression_parts.len() == 1
        }
        else {
            return false
        }
    }

    let mut cleaned = line;

    // skip lines that are defining a function
    if cleaned.contains("function") { return cleaned; }

    // get the nth index of the first open paren
    let nth_paren_index = match cleaned.match_indices("(").nth(paren_index) {
        Some(x) => x.0,
        None => return cleaned,
    };

    //find it's matching close paren
    let (paren_start, paren_end, found_match) = find_balanced_parentheses(cleaned[nth_paren_index..].to_string());

    // add the nth open paren to the start of the paren_start
    let paren_start = paren_start + nth_paren_index;
    let paren_end = paren_end + nth_paren_index;

    // if a match was found, check if the parens are unnecessary
    match found_match {
        true => {
            
            // get the logical expression including the char before the parentheses (to detect casts)
            let logical_expression = match paren_start {
                0 => match cleaned.get(paren_start..paren_end+1) {
                    Some(expression) => expression.to_string(),
                    None => cleaned[paren_start..paren_end].to_string(),
                },
                _ => match cleaned.get(paren_start - 1..paren_end+1) {
                    Some(expression) => expression.to_string(),
                    None => cleaned[paren_start - 1..paren_end].to_string(),
                }
            };

            // check if the parentheses are unnecessary and remove them if so
            if are_parentheses_unnecessary(logical_expression.clone()) {
                
                cleaned.replace_range(
                    paren_start..paren_end,
                    match logical_expression.get(2..logical_expression.len() - 2) {
                        Some(x) => x,
                        None => "",
                    }
                );

                // remove double negation, if one was created
                if cleaned.contains("!!") {
                    cleaned = cleaned.replace("!!", "");
                }

                // recurse into the next set of parentheses
                // don't increment the paren_index because we just removed a set
                cleaned = simplify_parentheses(cleaned, paren_index);
            }
            else {

                // recurse into the next set of parentheses
                cleaned = simplify_parentheses(cleaned, paren_index + 1);
            }
        },
        _ => {
            
            // if you're reading this you're a nerd
        }
    }

    cleaned
}

fn convert_iszero_logic_flip(line: String) -> String {
    let mut cleaned = line;

    if cleaned.contains("iszero") {
        cleaned = cleaned.replace("iszero", "!");
    }

    cleaned
}

pub fn postprocess(line: String) -> String {
    let mut cleaned = line;

    // Find and convert all castings
    cleaned = convert_bitmask_to_casting(cleaned);

    // Remove all repetitive casts
    cleaned = simplify_casts(cleaned);

    // Find and flip == / != signs for all instances of ISZERO
    cleaned = convert_iszero_logic_flip(cleaned);

    // Remove all unnecessary parentheses
    cleaned = simplify_parentheses(cleaned, 0);

    cleaned
}
