struct OrderedPropertyNames<S = rustc_hash::FxBuildHasher> {
    linear: Vec<String>,
    index: Option<std::collections::HashMap<String, usize, S>>,
}

impl<S> Default for OrderedPropertyNames<S> {
    fn default() -> Self {
        Self { linear: Vec::new(), index: None }
    }
}

impl<S: std::hash::BuildHasher + Default> OrderedPropertyNames<S> {
    fn insert(&mut self, name: String) {
        if self.index.is_none() && self.linear.len() == 32 {
            self.index = Some(
                std::mem::take(&mut self.linear)
                    .into_iter()
                    .enumerate()
                    .map(|(position, name)| (name, position))
                    .collect(),
            );
        }
        if let Some(index) = &mut self.index {
            let position = index.len();
            index.entry(name).or_insert(position);
        } else if !self.linear.contains(&name) {
            self.linear.push(name);
        }
    }

    fn into_vec(self) -> Vec<String> {
        let Some(index) = self.index else { return self.linear };
        let mut names = vec![String::new(); index.len()];
        for (name, position) in index {
            names[position] = name;
        }
        names
    }
}

fn main() {
 for width in [4usize,16,32,33,256,1024] {
  for degree in [1usize,4] {
   let mut linear:Vec<String>=Vec::new();
   let mut hybrid=OrderedPropertyNames::<rustc_hash::FxBuildHasher>::default();
   let mut copied_names=0;let mut copied_bytes=0;
   for base in 0..degree {
    let order:Vec<_>=(0..width).collect();
    let names:Vec<String>=order.iter().map(|i|format!("p{}",if base%2==0 {*i} else {width-1-*i})).collect();
    for name in names {
     copied_names+=1;copied_bytes+=name.len();
     if !linear.contains(&name) { linear.push(name.clone()); }
     hybrid.insert(name);
    }
   }
   let indexed=hybrid.index.is_some();
   let index_capacity=hybrid.index.as_ref().map_or(0,|v|v.capacity());
   let hybrid_linear_capacity=hybrid.linear.capacity();
   let linear_capacity=linear.capacity();
   let output=hybrid.into_vec();assert_eq!(linear,output);
   println!("{{\"width\":{width},\"degree\":{degree},\"owned_names_per_mode\":{copied_names},\"owned_payload_per_mode\":{copied_bytes},\"indexed\":{indexed},\"index_capacity\":{index_capacity},\"hybrid_linear_capacity_before_finish\":{hybrid_linear_capacity},\"linear_vector_capacity\":{linear_capacity},\"hybrid_output_capacity\":{},\"string_element_bytes\":{},\"index_entry_logical_bytes\":{}}}",output.capacity(),std::mem::size_of::<String>(),std::mem::size_of::<(String,usize)>());
  }
 }
}
