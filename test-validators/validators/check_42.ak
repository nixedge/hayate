validator check_42 {
  spend(_datum: Option<Data>, redeemer: Int, _output_reference: Data, _context: Data) {
    redeemer == 42
  }
}
