#![warn(clippy::all)]
#![allow(clippy::range_plus_one, clippy::many_single_char_names, clippy::too_many_arguments, clippy::cast_lossless, unused_variables)]

use grafix_toolbox::*;

use glsl::*;
use uses::{asyn::*, math::*, sync::fs, GL, *};
use GL::{mesh::*, offhand::*, window::*, *};

fn main() {
	std::env::set_var("SMOL_THREADS", (num_cpus::get_physical() * 2 + 2).to_string());
	LOGGER!(logging::Null, DEBUG);

	use clap::{App, SubCommand};
	let args = App::new("Finds duplicate images from blockhash list")
		.about(
			r#"
Install blockhash first: https://github.com/commonsmachinery/blockhash/
To prepare a list of blockhashes run:
find . -regextype posix-egrep -regex ".*\.(png|jpe?g)$" -type f -printf '"%p"\n' | xargs -IF -P8 blockhash F > hashes

Note that paths are relative and you must run dedup from the same directory as find"#,
		)
		.subcommand(
			SubCommand::with_name("find")
				.about("Prints all files with duplicates within a list of blockhashes. Prints filenames to strdout and progress to stderr, so redirect output to a file 'dedup_adv find hashes > dupes'")
				.args_from_usage(
					"<PATH>  'Path to the list of blockhashes'
					 [PATH2] 'If present duplicate wills be considered within PATH -> PATH2'
					 -s, --similarity=[SIMILARITY] 'Integer [1..100], cutoff similarity percentage'
					 -U, --ultra                   'Ultra precision, performs pixelwise feature comparisons'
					 -D, --display                 'Displays ultra precision diffs'",
				),
		)
		.subcommand(
			SubCommand::with_name("filter")
				.about("Filters duplicates from a list generated by dedup. Duplicates are judged on smaller resolution, otherwise alphabetically and if located deeper within filesystem")
				.args_from_usage(
					"<PATH> 'Path to the list of duplicates'
					 -D, --depth-first 'Force depth first. Will judge files based on directory names, filesystem depth, then on resolution'
					 -A, --append      'Append duplicates filenames to main file. WILL MANIPULATE FILESYSTEM!'",
				),
		)
		.get_matches();

	if let Some(args) = args.subcommand_matches("find") {
		run_search(args);
	} else if let Some(args) = args.subcommand_matches("filter") {
		run_filter(args);
	}
}

fn run_filter(args: &clap::ArgMatches) {
	let (depth_first, append, paths) = (args.is_present("depth-first"), args.is_present("append"), args.value_of("PATH").unwrap());
	let paths = FS::Load::Text(paths).expect(&format!("Couldn't open results file {}", paths));
	let paths = paths.lines().collect::<Vec<&str>>();
	FnStatic!(_paths, Vec<&'static str>, { paths.iter().map(|p| unsafe { mem::transmute(*p) }).collect() });
	let (mut start, mut end) = (0, 0);
	let tasks: Vec<_> = _paths()
		.iter()
		.enumerate()
		.filter_map(|(n, l)| {
			end += 1;
			if l.is_empty() {
				let t = task::spawn(async move {
					let end = end - 1;
					let mut sizes = HashMap::with_capacity(end - start);
					let mut dupes = _paths()[start..end].iter().copied().collect::<Vec<&str>>();
					let depth = |l: &str| Path::new(l).ancestors().count();
					let alph = |l: &str| Path::new(l).ancestors().skip(1).map(|p| p.to_str().unwrap().to_string()).collect::<Vec<String>>();
					fn size<'a>(sizes: &mut HashMap<&'a str, usize>, p: &'a str) -> usize {
						*sizes.entry(p).or_insert(imagesize::size(p).ok().map_or_else(
							|| {
								eprintln!("Failed to determine size of {:?}", p);
								0
							},
							|size| size.width * size.height,
						))
					}
					dupes.sort_unstable_by(|l, r| {
						let depth = || {
							alph(l)
								.iter()
								.rev()
								.zip(alph(r).iter().rev())
								.find_map(|(l, r)| Some(l.cmp(r)).filter(|c| *c != ord::Equal))
								.unwrap_or(depth(l).cmp(&depth(r)))
						};

						let mut size = || size(&mut sizes, r).cmp(&size(&mut sizes, l));
						if depth_first {
							depth().then(size()).reverse()
						} else {
							size().then(depth()).reverse()
						}
					});
					let (base, dupes) = dupes.split_last().unwrap();
					(|| {
						if append {
							let base = Path::new(base);
							let previous = base.to_owned();
							let (path, mut base, ext) = (base.parent()?, base.file_stem()?.to_str()?.to_string(), base.extension()?);
							base.push('_');
							for n in dupes {
								base.push_str(Path::new(n).file_stem()?.to_str()?);
							}
							let max_len = 247 - ext.to_str()?.len();
							let base: String = base.char_indices().take_while(|(i, _)| *i < max_len).map(|(_, c)| c).collect();
							let path = path.join(format!("{}.{}", &base, ext.to_str()?));
							if let Err(e) = fs::rename(&previous, &path) {
								eprintln!("Could not rename base file {:?} to {:?}, err {}", previous, path, e);
							}
						}
						Some(())
					})()
					.unwrap();
					dupes.iter().for_each(|d| println!("{}", d));
				});

				start = end;
				Some(t)
			} else {
				None
			}
		})
		.collect();

	task::block_on(async move {
		for t in tasks {
			t.await
		}
	});
}

fn run_search(args: &clap::ArgMatches) {
	let (num_cpus, ultra_precision, show_differences, precision, paths, paths2) = (
		num_cpus::get_physical(),
		args.is_present("ultra"),
		args.is_present("display"),
		args.value_of("similarity").and_then(|a| a.parse().ok()).unwrap_or(88).min(100),
		args.value_of("PATH").unwrap(),
		args.value_of("PATH2"),
	);

	type Names = HashMap<usize, &'static str>;
	fn parse_paths<'a>(paths: &str) -> (Vec<u8>, Names) {
		let mut hashes = vec![];
		let names = paths
			.lines()
			.enumerate()
			.map(|(n, l)| {
				let n = n * 32;
				let (hash, name) = l.split_once(char::is_whitespace).expect(&format!("Invalid input {:?}", (n, l)));
				let (hash, name) = (hash.trim_end(), name.trim_start());
				hashes.extend((0..32).map(|i| {
					let i = 2 * i;
					u8::from_str_radix(&hash[i..i + 2], 16).expect("Invalid blockhash list formatting")
				}));

				let h = &mut hashes[n];
				*h = h.or_val(*h != 0, 1);
				(n, unsafe { mem::transmute(name) })
			})
			.collect();
		(hashes, names)
	}

	let paths = FS::Load::Text(paths).expect(&format!("Couldn't open blockhash list file {}", paths));
	let (h1, names1) = parse_paths(&paths);
	FnStatic!(hashes1, Vec<u8>, { h1 });
	let paths2 = paths2.map(|p| FS::Load::Text(p).expect(&format!("Couldn't open blockhash list file {}", p)));
	let (mut hashes2, names2) = if let Some((h, n)) = paths2.as_ref().map(|p| parse_paths(&p)) { (h, n) } else { Def() };

	let two_sets = !hashes2.is_empty();
	let ultra_precision = ultra_precision && precision > 89;
	let show_differences = ultra_precision && show_differences;
	let thresh = u64::to(256. * (1. - precision as f32 / 100.));

	let (hashes2, names2) = if two_sets { (&mut hashes2, names2) } else { (hashes1(), names1.clone()) };
	let (total1, total2) = (names1.len() * 32, names2.len() * 32);
	let (names2, mut hashes2) = StaticPtr!(&names2, hashes2);

	let mut window = Window::get((50, 50, 64, 64), "Engine").expect("Can't start GL");
	GLDisable!(DEPTH_TEST, BLEND, MULTISAMPLE, CULL_FACE, DEPTH_WRITEMASK);

	let mut subs = Shader::new((mesh__2d_screen_vs, substract_ps)).expect("Can't create shader");
	let mut render = Shader::new((mesh__2d_screen_vs, mesh__2d_screen_ps)).expect("Can't create shader");
	let linear = &Sampler::linear();
	let imgify = |data: uImage<_>| Tex2d::<RGBA, u8>::from(data);
	let (offhand_sn, offhand_rx) = Offhand::new(&mut window, 64, move |p: Option<_>| p.map(|(n, name, data)| (n, name, imgify(data))));

	for i1 in (0..total1).step_by(32) {
		if !two_sets && *unsafe { hashes1().get_unchecked(i1) } == 0 {
			continue;
		}

		let dupe = AtomicBool::default();
		let base_name = names1.get(&i1).unwrap();

		{
			let dupe = StaticPtr!(&dupe);

			let sn = offhand_sn.clone();
			let base = unsafe { hashes1().get_unchecked(i1..i1 + 32) };

			let mut tasks = vec![];
			tasks.push(task::spawn(async move {
				let mut workers = vec![];

				let start = i1.or_def(!two_sets);
				let step = 32 * 1.max((total2 - start) / num_cpus / 4 / 32);

				for i in (start..total2).step_by(step) {
					let snl = sn.clone();
					let range = i..total2.min(i + step);
					let w = task::spawn(async move {
						let dupe = dupe.get();
						let (names2, hashes2) = (names2.get(), hashes2.get_mut());
						for i2 in range.step_by(32) {
							if *unsafe { hashes2.get_unchecked(i2) } == 0 || (!two_sets && i1 == i2) {
								continue;
							}

							let diff = { hamming::distance_fast(base, unsafe { hashes2.get_unchecked(i2..i2 + 32) }).unwrap() };

							if diff < thresh {
								let name = names2.get(&i2).unwrap();
								if !ultra_precision {
									dupe.store(true, Ordering::Release);
									*unsafe { hashes2.get_unchecked_mut(i2) } = 0;
									println!("{}", name);
								} else {
									if let Ok(f) = FS::Load::File(name).and_then(|f| uImage::new(f)) {
										let _ = snl.send_async(Some((i2, name, f))).await.unwrap();
									} else {
										eprintln!("could not open copy {}", name);
									}
								}
							}
						}
					});
					workers.push(w);
				}

				for w in workers {
					w.await
				}

				if ultra_precision {
					let _ = sn.send(None).unwrap();
				}
			}));
			if ultra_precision {
				let mut base_file = Some(FS::Preload::File(base_name));
				let mut base = None;
				while let Some((i2, name, recv)) = offhand_rx.recv().wait().unwrap() {
					if let Some(f) = base_file.take() {
						if let Ok(b) = uImage::new(f.take()) {
							base = Some(imgify(b));
						} else {
							eprintln!("could not open base {}", base_name);
						}
					}
					let base = if let Some(base) = base.as_ref() {
						base
					} else {
						continue;
					};

					let s1 = base.Bind(linear);
					let s2 = recv.Bind(linear);
					let size = {
						let s = base.param;
						(s.w, s.h).div(2)
					};

					let data = {
						let out = {
							let _ = Uniforms!(subs, ("tex1", &s1), ("tex2", &s2), ("size", ((1., 1.).div(size))));
							let mut out = Fbo::<RGB, u8>::new(size);
							out.bind();
							Screen::Draw();
							if show_differences {
								window.draw_to_screen();
								GL::ClearScreen((0., 1.));
								let b = out.tex.Bind(linear);
								let _ = Uniforms!(render, ("tex", &b));
								Screen::Draw();
								let _ = window.poll_events();
								window.swap();
							}
							out.tex
						};
						let b = out.Bind(linear);
						let _ = Uniforms!(render, ("tex", &b));
						let mut out = Fbo::<RGB, u8>::new((256, 256));
						out.bind();
						Screen::Draw();
						out.tex.Save::<RED, u8>(0)
					};

					tasks.push(task::spawn(async move {
						let (dupe, hashes2) = (dupe.get(), hashes2.get_mut());
						let base = data.iter().fold(0., |acc, v| acc + (*v as f32)) / data.len() as f32;
						let diff = data
							.iter()
							.fold(0., |acc, v| {
								let v = *v as f32;
								let d = (base - v).powi(2);
								acc + d
							})
							.sqrt();
						data.len() as f32;
						if diff < 1. + 10. * (100 - precision) as f32 {
							dupe.store(true, Ordering::Release);
							*unsafe { hashes2.get_unchecked_mut(i2) } = 0;
							println!("{}", name);
						}
						//println!("{} {:?}", name, (/*data, */ diff, base));
					}));
				}
			}
			task::block_on(async move {
				for t in tasks {
					t.await
				}
			});
		}

		if dupe.load(Ordering::Acquire) {
			println!("{}\n", base_name);
		}

		if i1 % 100 == 0 {
			eprintln!("processed {}% files", i1 as f32 * 100. / total1 as f32);
		}
	}
	drop(offhand_sn);
}

SHADER!(
	substract_ps,
	r"#version 330 core
	in vec2 glTexCoord;
	layout(location = 0)out vec4 glFragColor;
	uniform sampler2D tex1, tex2;
	uniform vec2 size;

	#define edge2 mat3(-1, -1, -1, -1, 8, -1, -1, -1, -1)

	float pix(sampler2D t) {
		vec3 p = texture(t, glTexCoord).rgb;
		return (p.r + p.g + p.b) * 0.333;
	}

	float val(float x, float y, sampler2D t) {
		vec3 c = texture(t, glTexCoord + vec2(x, y) * size).rgb;
		return round(10 * (c.r + c.g + c.b)) * 0.0333;
	}

	mat3 reg(sampler2D t) {
		return mat3(val(-1, -1, t), val(0, -1, t), val(1, -1, t),
					val(-1,  0, t), val(0,  0, t), val(1,  0, t),
					val(-1,  1, t), val(0,  1, t), val(1,  1, t));
	}

	float edg(sampler2D t) {
		mat3 r = matrixCompMult(edge2, reg(t));
		return abs(r[0][0] + r[1][0] + r[2][0]
				 + r[0][1] + r[1][1] + r[2][1]
				 + r[0][2] + r[1][2] + r[2][2]);
	}

	float mag(sampler2D t) {
		mat3 r = reg(t);
		return max(max(max(r[0][0], r[1][0]), max(r[2][0], r[0][1])),
			   max(max(max(r[1][1], r[2][1]), max(r[0][2], r[1][2])), r[2][2]));
	}

	void main()
	{
		float p1 = edg(tex1);
		float p2 = edg(tex2);
		float c1 = pix(tex1);
		float c2 = pix(tex2);
		float c = abs(c1 - c2);
		c = c * float(c > 0.1);
		float max_p1 = mag(tex1);
		float max_p2 = mag(tex2);
		float d = abs(p1 - p2);
		d = d * float(d > 0.2);
		d = max(0, max(d - max_p1, d - max_p2));
		glFragColor = vec4(d * d * d + c, 0, 0, 1);
	}"
);
