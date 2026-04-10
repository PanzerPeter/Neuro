import re
import os

def extract_methods(content, start_pos):
    methods = {}
    pos = start_pos
    
    # find `impl TypeChecker {`
    impl_start = content.find("impl TypeChecker {", pos)
    if impl_start == -1: return methods
    
    pos = impl_start + len("impl TypeChecker {")
    end_of_impl = -1
    
    while True:
        # find `fn name(`
        # or `pub(crate) fn name(`
        match = re.search(r'(?:pub\(crate\)\s+)?fn\s+([a-zA-Z0-9_]+)\s*(?:<[^>]+>)?\s*\(', content[pos:])
        if not match:
            # Look for the end of impl `}`
            break
            
        fn_name = match.group(1)
        fn_start = pos + match.start()
        
        # Before fn_start, there might be docs/attributes. Let's include them.
        docs_start = fn_start
        while docs_start > pos:
            # check the line backward
            line_start = content.rfind('\n', pos, docs_start - 1)
            if line_start == -1: line_start = pos
            line = content[line_start:docs_start].strip()
            if line.startswith('///') or line.startswith('#[') or line == '':
                docs_start = line_start
            else:
                break
                
        # Now find the matching brace for the function
        brace_start = content.find('{', fn_start)
        if brace_start == -1: break
        
        brace_count = 1
        fn_end = brace_start + 1
        while brace_count > 0 and fn_end < len(content):
            if content[fn_end] == '{':
                brace_count += 1
            elif content[fn_end] == '}':
                brace_count -= 1
            fn_end += 1
            
        methods[fn_name] = content[docs_start:fn_end]
        pos = fn_end

    return methods, impl_start

def main():
    with open('compiler/semantic-analysis/src/type_checker.rs', 'r') as f:
        content = f.read()

    # Get header imports and struct definition
    impl_start = content.find("impl TypeChecker {")
    preamble = content[:impl_start]
    
    methods, _ = extract_methods(content, 0)
    
    tests_start = content.find("#[cfg(test)]")
    if tests_start != -1:
        postamble = content[tests_start:]
    else:
        postamble = ""

    os.makedirs('compiler/semantic-analysis/src/type_checkers/tests', exist_ok=True)
    
    # Write mod.rs
    with open('compiler/semantic-analysis/src/type_checkers/mod.rs', 'w') as f:
        f.write(preamble)
        f.write("mod literals;\nmod resolution;\nmod expressions;\nmod statements;\nmod declarations;\n\n")
        f.write("#[cfg(test)]\nmod tests;\n\n")
        f.write("impl TypeChecker {\n")
        for name in ['new', 'record_error', 'into_errors', 'has_errors', 'check_program']:
            if name in methods:
                f.write(methods[name] + "\n")
        f.write("}\n")
        
    # Write literals.rs
    with open('compiler/semantic-analysis/src/type_checkers/literals.rs', 'w') as f:
        f.write("use super::TypeChecker;\nuse crate::types::Type;\nuse shared_types::Span;\nuse crate::errors::TypeError;\n\nimpl TypeChecker {\n")
        for name in ['check_integer_range', 'infer_integer_type', 'infer_float_type']:
            if name in methods:
                f.write(methods[name] + "\n")
        f.write("}\n")

    # Write resolution.rs
    with open('compiler/semantic-analysis/src/type_checkers/resolution.rs', 'w') as f:
        f.write("use super::TypeChecker;\nuse crate::types::Type;\nuse crate::errors::TypeError;\n\nimpl TypeChecker {\n")
        for name in ['resolve_type']:
            if name in methods:
                f.write(methods[name] + "\n")
        f.write("}\n")
        
    # Write expressions.rs
    with open('compiler/semantic-analysis/src/type_checkers/expressions.rs', 'w') as f:
        f.write("use super::TypeChecker;\nuse crate::types::Type;\nuse crate::errors::TypeError;\nuse ast_types::{Expr, BinaryOp, UnaryOp};\nuse shared_types::Literal;\n\nimpl TypeChecker {\n")
        for name in ['check_plain_call', 'check_expr']:
            if name in methods:
                f.write(methods[name] + "\n")
        f.write("}\n")
        
    # Write statements.rs
    with open('compiler/semantic-analysis/src/type_checkers/statements.rs', 'w') as f:
        f.write("use super::TypeChecker;\nuse crate::types::Type;\nuse crate::errors::TypeError;\nuse ast_types::Stmt;\nuse shared_types::Identifier;\n\nimpl TypeChecker {\n")
        for name in ['check_stmt']:
            if name in methods:
                f.write(methods[name] + "\n")
        f.write("}\n")
        
    # Write declarations.rs
    with open('compiler/semantic-analysis/src/type_checkers/declarations.rs', 'w') as f:
        f.write("use super::TypeChecker;\nuse crate::types::Type;\nuse crate::errors::TypeError;\nuse ast_types::{FunctionDef, StructDef, ImplDef, SelfParam};\n\nimpl TypeChecker {\n")
        for name in ['check_function', 'register_struct', 'register_impl', 'check_impl']:
            if name in methods:
                f.write(methods[name] + "\n")
        f.write("}\n")

    print('Split done')

if __name__ == '__main__':
    main()
