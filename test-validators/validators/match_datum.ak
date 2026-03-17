validator match_datum {
  spend(datum_opt: Option<Int>, redeemer: Int, _output_reference: Data, _context: Data) {
    when datum_opt is {
      Some(datum) -> datum == redeemer
      None -> False
    }
  }
}
