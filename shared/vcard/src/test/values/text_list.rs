use crate::values::text::Text;
use crate::values::text_list::TextList;

#[test]
fn text_list_struct() {
    let text_list = TextList::from(r"\\ \, \n 	 𝕯!+-[]~,\\ \, \n 	 𝕯!+-[]~");
    assert_eq!(text_list.0.len(), 2);
    assert_eq!(
        text_list.0[0],
        Text::new(
            r"\ , 
 	 𝕯!+-[]~"
        )
    );
    assert_eq!(
        text_list.0[1],
        Text::new(
            r"\ , 
 	 𝕯!+-[]~"
        )
    );
}
