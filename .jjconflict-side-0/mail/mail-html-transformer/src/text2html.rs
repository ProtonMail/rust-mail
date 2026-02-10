use kuchikiki::NodeRef;
use std::iter::empty;

pub fn text2html(text: &str) -> NodeRef {
    let document = NodeRef::new_document();
    let body = crate::utils::new_element::<&str, &str>("body", empty());
    document.append(body.clone());

    let mut lines = text.lines().map(str::trim);

    if let Some(first_line) = lines.next() {
        // We do not wrap first line in a div because user may paste
        // text in the middle of the line.
        body.append(process_line(first_line));
    }
    for line in lines {
        let div = crate::utils::new_element::<&str, &str>("div", empty());
        div.append(process_line(line));
        body.append(div);
    }

    document
}

// Matching behaviour of ProtonMail Web
fn process_line(line: &str) -> NodeRef {
    if line.is_empty() {
        return crate::utils::new_element::<&str, &str>("br", empty());
    }

    let span = crate::utils::new_element::<&str, &str>("span", empty());
    span.append(NodeRef::new_text(line));
    span
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plaintext_to_html() {
        // Generated with: https://www.blindtextgenerator.com/lorem-ipsum
        let input = "Lorem ipsum dolor sit amet, consectetuer adipiscing elit. Aenean commodo ligula eget dolor. Aenean massa. Cum sociis natoque penatibus et magnis dis parturient montes, nascetur ridiculus mus. Donec quam felis, ultricies nec, pellentesque eu, pretium quis, sem. Nulla consequat massa quis enim. Donec pede justo, fringilla vel, aliquet nec, vulputate eget, arcu.

        In enim justo, rhoncus ut, imperdiet a, venenatis vitae, justo. Nullam dictum felis eu pede mollis pretium. Integer tincidunt. Cras dapibus. Vivamus elementum semper nisi. Aenean vulputate eleifend tellus. Aenean leo ligula, porttitor eu, consequat vitae, eleifend ac, enim. Aliquam lorem ante, dapibus in, viverra quis, feugiat a, tellus.

        Phasellus viverra nulla ut metus varius laoreet. Quisque rutrum. Aenean imperdiet. Etiam ultricies nisi vel augue. Curabitur ullamcorper ultricies nisi. Nam eget dui. Etiam rhoncus. Maecenas tempus, tellus eget condimentum rhoncus, sem quam semper libero, sit amet adipiscing sem neque sed ipsum. Nam quam nunc, blandit vel, luctus pulvinar, hendrerit id, lorem. Maecenas nec odio et ante tincidunt tempus. Donec vitae sapien ut libero venenatis faucibus. Nullam quis ante. Etiam sit amet orci eget eros faucibus tincidunt. Duis leo. Sed fringilla mauris sit amet nibh. Donec sodales sagittis magna. Sed consequat, leo eget bibendum sodales, augue velit cursus nunc,";

        let document = text2html(input);

        let output = document.to_string();

        insta::assert_snapshot!(output);
    }

    #[test]
    fn plaintext_with_html_in_it() {
        let input = "<b>Hallo</b>";
        let document = text2html(input);

        let output = document.to_string();

        insta::assert_snapshot!(output);
    }

    #[test]
    fn plaintext_with_extra_spaces_in_it() {
        // Note this is a difference between text2html and
        // keep_spaces_and_escape_gt_and_lt.
        // The latter would replace space just before `d` with `&nbsp;`
        let input = "Hallo  double spaces";
        let document = text2html(input);

        let output = document.to_string();

        insta::assert_snapshot!(output);
    }

    #[test]
    fn plaintext_newlines_in_the_same_paragraph() {
        let input = "Hallo\nWorld\n\nToday";
        let document = text2html(input);

        let output = document.to_string();

        insta::assert_snapshot!(output);
    }
}
