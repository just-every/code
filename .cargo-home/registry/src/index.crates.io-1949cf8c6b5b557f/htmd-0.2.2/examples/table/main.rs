use htmd::convert;

fn main() {
    let html = r#"
    <table>
        <thead>
            <tr>
                <th>Programming Language</th>
                <th>Type</th>
                <th>Release Year</th>
            </tr>
        </thead>
        <tbody>
            <tr>
                <td>Rust</td>
                <td>Systems</td>
                <td>2010</td>
            </tr>
            <tr>
                <td>Python</td>
                <td>Interpreted</td>
                <td>1991</td>
            </tr>
            <tr>
                <td>JavaScript</td>
                <td>Interpreted</td>
                <td>1995</td>
            </tr>
        </tbody>
    </table>
    "#;

    match convert(html) {
        Ok(markdown) => println!("Converted Markdown:\n{}", markdown),
        Err(e) => eprintln!("Error converting HTML: {}", e),
    }
}