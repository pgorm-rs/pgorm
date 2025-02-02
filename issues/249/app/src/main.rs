use service::clone_a_model;

use pgorm::tests_cfg::cake;

fn main() {
	let c1 = cake::Model {
		id: 1,
		name: "Cheese".to_owned(),
	};

	let c2 = clone_a_model(&c1);

	println!("{:?}", c2);
}