/// A test context.
#[non_exhaustive]
pub struct Context {
    /// The complete module test path  
    pub module: &'static str,
    /// The test function name  
    pub name: &'static str,
    /// The test description if present
    pub description: Option<&'static str>,
    /// The cardinal case number if it's a test case
    pub case: Option<usize>,
    /// Start time
    pub start: std::time::Instant,
}

impl Context {
    /// Create a new test context. This function set also the start time to the current time.
    pub fn new(
        module: &'static str,
        name: &'static str,
        description: Option<&'static str>,
        case: Option<usize>,
    ) -> Self {
        Self {
            module,
            name,
            description,
            case,
            start: std::time::Instant::now(),
        }
    }
}
