#[cfg(test)]
mod table_tests_1 {
    use htmd::convert;

    #[test]
    fn test_simple_table() {
        let html = r#"
        <table>
            <tr>
                <th>Header 1</th>
                <th>Header 2</th>
            </tr>
            <tr>
                <td>Cell 1</td>
                <td>Cell 2</td>
            </tr>
            <tr>
                <td>Cell 3</td>
                <td>Cell 4</td>
            </tr>
        </table>
        "#;

        let expected = r#"
| Cell 1 | Cell 2 |
| Cell 3 | Cell 4 |
"#
        .trim();

        let markdown = convert(html).unwrap();
        let result = markdown.trim();
        assert_eq!(expected, result);
    }

    #[test]
    fn test_table_with_thead_tbody() {
        let html = r#"
        <table>
            <thead>
                <tr>
                    <th>Name</th>
                    <th>Age</th>
                    <th>Location</th>
                </tr>
            </thead>
            <tbody>
                <tr>
                    <td>John</td>
                    <td>35</td>
                    <td>New York</td>
                </tr>
                <tr>
                    <td>Jane</td>
                    <td>28</td>
                    <td>San Francisco</td>
                </tr>
            </tbody>
        </table>
        "#;

        let expected = r#"
| Name | Age | Location      |
| ---- | --- | ------------- |
| John | 35  | New York      |
| Jane | 28  | San Francisco |
"#
        .trim();

        let markdown = convert(html).unwrap();
        let result = markdown.trim();
        assert_eq!(expected, result);
    }

    #[test]
    fn test_table_with_caption() {
        let html = r#"
        <table>
            <caption>Sample Table</caption>
            <tbody>
                <tr>
                    <td>John</td>
                    <td>35</td>
                    <td>New York</td>
                </tr>
                <tr>
                    <td>Jane</td>
                    <td>28</td>
                    <td>San Francisco</td>
                </tr>
            </tbody>
        </table>
        "#;

        let expected = r#"
Sample Table
| John | 35 | New York      |
| Jane | 28 | San Francisco |
"#
        .trim();

        let markdown = convert(html).unwrap();
        let result = markdown.trim();
        assert_eq!(expected, result);
    }

    #[test]
    fn test_empty_table() {
        let html = "<table></table>";
        let markdown = convert(html).unwrap();
        let result = markdown.trim();
        assert_eq!("", result);
    }
}
